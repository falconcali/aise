use crate::config::{AssetLimitsConfig, ContextPreparationConfig, TurnContentLimitsConfig};
use crate::context::error::ContextError;
use crate::context::retrieval_signal_builder::RetrievalSignalBuilder;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{BaselineContext, CharacterIndexEntry, CharacterView, NarrativeStateView, SnapshotLimits};
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use crate::domain::narrative::StoryContinuityLimits;
use crate::persistence::store::Store;
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

pub struct BaselineContextBuilder {
    store: Arc<dyn Store>,
    content_limits: TurnContentLimitsConfig,
    context_config: ContextPreparationConfig,
    asset_limits: AssetLimitsConfig,
    signal_builder: RetrievalSignalBuilder,
}

impl BaselineContextBuilder {
    pub fn new(
        store: Arc<dyn Store>,
        content_limits: TurnContentLimitsConfig,
        context_config: ContextPreparationConfig,
        asset_limits: AssetLimitsConfig,
    ) -> Self {
        let signal_builder = RetrievalSignalBuilder::new(context_config.clone());
        Self {
            store,
            content_limits,
            context_config,
            asset_limits,
            signal_builder,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for BaselineContextBuilder {
    fn stage(&self) -> TurnStage {
        TurnStage::BaselineBuilder
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let story_id = ctx.story_id().clone();
        let limits = SnapshotLimits::from_config(&self.content_limits, &self.context_config, &self.asset_limits);
        let snapshot = {
            let pending = ctx.trace().begin_span("aise.tool_call", "store.load_story_snapshot");
            let started = Instant::now();
            let outcome = self.store.load_story_snapshot(&story_id, limits).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(snapshot) => (true, serde_json::json!({ "revision": snapshot.base_revision().get() })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace().end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_story_snapshot".into(),
                    args: serde_json::json!({ "story_id": story_id.to_string() }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome.map_err(TurnExecutionError::from)?
        };
        let continuity_limits = StoryContinuityLimits {
            max_summary_bytes: limits.continuity.max_summary_bytes,
            max_recent_segments: limits.continuity.max_recent_segments,
            max_recent_segment_bytes: limits.continuity.max_recent_segment_bytes,
            max_recent_segment_tokens: limits.continuity.max_recent_segment_tokens,
        };
        let _ = continuity_limits;
        let pending = ctx.trace().begin_span("context.prepare", "context.prepare");
        let baseline_result = build_baseline(&snapshot, ctx.player_input(), &self.signal_builder, &self.context_config);
        let payload = match &baseline_result {
            Ok(baseline) => serde_json::json!({
                "story_id": story_id,
                "turn_id": ctx.turn_id(),
                "base_revision": snapshot.base_revision().get(),
                "character_count": baseline.character_index.len() + baseline.scene_characters.len() + 1,
                "constraint_count": baseline.active_story_constraints.len(),
                "entity_signal_count": baseline.retrieval_signals.entities.len(),
                "topic_signal_count": baseline.retrieval_signals.topics.len(),
                "status": "ok",
                "error_code": null,
            }),
            Err(error) => serde_json::json!({
                "story_id": story_id,
                "turn_id": ctx.turn_id(),
                "base_revision": snapshot.base_revision().get(),
                "character_count": 0,
                "constraint_count": 0,
                "entity_signal_count": 0,
                "topic_signal_count": 0,
                "status": "error",
                "error_code": error.turn_code(),
            }),
        };
        ctx.trace().end_span_with(pending, &payload);
        let baseline = baseline_result.map_err(map_baseline_error)?;
        ctx.set_prepared_context(snapshot, baseline)
    }
}

fn build_baseline(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    player_input: &str,
    signal_builder: &RetrievalSignalBuilder,
    context_config: &ContextPreparationConfig,
) -> Result<BaselineContext, ContextError> {
    let player_bindings: Vec<_> = snapshot
        .role_bindings()
        .values()
        .filter(|binding| binding.is_player_controlled())
        .collect();
    if player_bindings.len() != 1 {
        return Err(ContextError::SnapshotInconsistent {
            code: "player_binding_count",
        });
    }
    let player_binding = player_bindings[0];
    let player_character = resolve_character_view(snapshot, &player_binding.character_id)?;
    let mut seen_scene = BTreeSet::new();
    let mut scene_characters = Vec::new();
    for character_id in &snapshot.current_scene().present_character_ids {
        if !seen_scene.insert(character_id.clone()) {
            return Err(ContextError::SnapshotInconsistent {
                code: "duplicate_scene_character",
            });
        }
        scene_characters.push(resolve_character_view(snapshot, character_id)?);
        if scene_characters.len() > context_config.max_scene_characters {
            return Err(ContextError::SignalLimitExceeded {
                limit: "max_scene_characters",
            });
        }
    }
    let mut character_index = Vec::new();
    for (character_id, state) in snapshot.character_states() {
        if seen_scene.contains(character_id) {
            continue;
        }
        let role = snapshot
            .role_definitions()
            .get(&state.role_key)
            .ok_or(ContextError::SnapshotInconsistent {
                code: "missing_role_definition",
            })?;
        let card = snapshot
            .character_cards()
            .get(character_id)
            .ok_or(ContextError::SnapshotInconsistent {
                code: "missing_character_card",
            })?;
        let binding = snapshot
            .role_bindings()
            .values()
            .find(|binding| &binding.character_id == character_id)
            .ok_or(ContextError::SnapshotInconsistent {
                code: "missing_role_binding",
            })?;
        character_index.push(CharacterIndexEntry {
            character_id: character_id.clone(),
            role_key: state.role_key.clone(),
            name: card.meta.name.clone(),
            narrative_function: role.narrative_function.clone(),
            location_key: state.location.clone(),
            player_controlled: binding.is_player_controlled(),
        });
    }
    character_index.sort_by(|left, right| left.character_id.cmp(&right.character_id));
    if character_index.len() > context_config.max_character_index {
        return Err(ContextError::SignalLimitExceeded {
            limit: "max_character_index",
        });
    }
    let retrieval_signals = signal_builder.build(snapshot, player_input)?;
    Ok(BaselineContext {
        story_profile: snapshot.story_profile().clone(),
        instance_settings: snapshot.instance_settings().clone(),
        player_character,
        current_scene: snapshot.current_scene().clone(),
        scene_characters,
        character_index,
        story_continuity: snapshot.story_continuity().clone(),
        active_story_constraints: snapshot.active_constraints().to_vec(),
        narrative_state_view: NarrativeStateView {
            pack_digest: snapshot.pack().digest.clone(),
            graph_revision: snapshot.graph_revision(),
            node_states: snapshot.narrative_state().node_states.clone(),
        },
        retrieval_signals,
    })
}

fn resolve_character_view(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    character_id: &crate::domain::ids::CharacterId,
) -> Result<CharacterView, ContextError> {
    let state = snapshot
        .character_states()
        .get(character_id)
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_character_state",
        })?;
    let role = snapshot
        .role_definitions()
        .get(&state.role_key)
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_role_definition",
        })?;
    let binding = snapshot
        .role_bindings()
        .values()
        .find(|binding| &binding.character_id == character_id)
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_role_binding",
        })?
        .clone();
    let card = snapshot
        .character_cards()
        .get(character_id)
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_character_card",
        })?;
    Ok(CharacterView {
        character_id: character_id.clone(),
        role_key: state.role_key.clone(),
        role: role.clone(),
        binding,
        card: card.clone(),
        state: state.clone(),
    })
}

fn map_baseline_error(error: ContextError) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        error.turn_code(),
        Some(TurnStage::BaselineBuilder),
        error.to_string(),
    )
}
