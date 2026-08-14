use crate::config::{AssetLimitsConfig, ContextPreparationConfig, RetrievalConfig, TurnContentLimitsConfig};
use crate::context::error::ContextError;
use crate::context::retrieval_signal_builder::RetrievalSignalBuilder;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::{KnowledgeIndexMatch, KnowledgeKind};
use crate::domain::narrative::StoryContinuityLimits;
use crate::domain::turn::{
    BaselineContext, CharacterIndexEntry, CharacterView, KnowledgeEntryIndexEntry, NarrativeGraphStateIndex,
    RelevantKnowledge, RetrievalAudience, RetrievalIndexScope, RetrievalTargetId, SnapshotLimits,
};
use crate::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeFilter, KnowledgeIndexQuery, KnowledgeLookupHit, KnowledgeReadPort,
    TopicKnowledgeQuery,
};
use crate::persistence::store::Store;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_trace::{SpanPayload, ToolCallData};
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

pub struct BaselineContextBuilder {
    store: Arc<dyn Store>,
    content_limits: TurnContentLimitsConfig,
    context_config: ContextPreparationConfig,
    asset_limits: AssetLimitsConfig,
    narrative_limits: crate::config::NarrativeConfig,
    retrieval_config: RetrievalConfig,
    knowledge: Arc<dyn KnowledgeReadPort>,
    signal_builder: RetrievalSignalBuilder,
}

impl BaselineContextBuilder {
    pub fn new(
        store: Arc<dyn Store>,
        content_limits: TurnContentLimitsConfig,
        context_config: ContextPreparationConfig,
        asset_limits: AssetLimitsConfig,
        narrative_limits: crate::config::NarrativeConfig,
        retrieval_config: RetrievalConfig,
        knowledge: Arc<dyn KnowledgeReadPort>,
    ) -> Self {
        let signal_builder = RetrievalSignalBuilder::new(context_config.clone());
        Self {
            store,
            content_limits,
            context_config,
            asset_limits,
            narrative_limits,
            retrieval_config,
            knowledge,
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
        let limits = SnapshotLimits::from_config(
            &self.content_limits,
            &self.context_config,
            &self.asset_limits,
            &self.narrative_limits,
        );
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
        let baseline_result = build_baseline(
            &snapshot,
            ctx.player_input(),
            &self.signal_builder,
            &self.context_config,
            &self.retrieval_config,
            &self.knowledge,
        )
        .await;
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

async fn build_baseline(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    player_input: &str,
    signal_builder: &RetrievalSignalBuilder,
    context_config: &ContextPreparationConfig,
    retrieval_config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
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
    let mut seen_scene = BTreeSet::from([player_character.character_id.clone()]);
    let mut scene_characters = Vec::new();
    for character_id in &snapshot.current_scene().present_character_ids {
        if character_id == &player_character.character_id {
            continue;
        }
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
    let retrieval_signals = signal_builder.build(snapshot, player_input)?;
    let referenced_ids = referenced_character_ids(snapshot, &retrieval_signals, &seen_scene);
    let mut referenced_characters = Vec::new();
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
        if referenced_ids.contains(character_id) {
            referenced_characters.push(resolve_character_view(snapshot, character_id)?);
        } else {
            character_index.push(CharacterIndexEntry {
                character_id: character_id.clone(),
                role_key: state.role_key.clone(),
                name: card.meta.name.clone(),
                narrative_function: role.narrative_function.clone(),
                location_key: state.location.clone(),
                player_controlled: binding.is_player_controlled(),
            });
        }
    }
    referenced_characters.sort_by(|left, right| left.character_id.cmp(&right.character_id));
    character_index.sort_by(|left, right| left.character_id.cmp(&right.character_id));
    let character_index_scope = if character_index.len() > context_config.max_character_index {
        character_index.truncate(context_config.max_character_index);
        RetrievalIndexScope::Prefiltered
    } else {
        RetrievalIndexScope::Complete
    };
    let relevant_knowledge = load_relevant_knowledge(snapshot, &retrieval_signals, retrieval_config, knowledge).await?;
    let (knowledge_entry_index_scope, knowledge_entry_index) =
        load_knowledge_index(snapshot, &relevant_knowledge, retrieval_config, knowledge).await?;
    Ok(BaselineContext {
        story_profile: snapshot.story_profile().clone(),
        instance_settings: snapshot.instance_settings().clone(),
        player_character,
        current_scene: snapshot.current_scene().clone(),
        scene_characters,
        referenced_characters,
        relevant_knowledge,
        character_index_scope,
        knowledge_entry_index_scope,
        knowledge_entry_index,
        character_index,
        story_continuity: snapshot.story_continuity().clone(),
        active_story_constraints: snapshot.active_constraints().to_vec(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: snapshot.pack().digest.clone(),
            graph_revision: snapshot.graph_revision(),
            node_states: snapshot.narrative_state().node_states.clone(),
        },
        retrieval_signals,
    })
}

fn referenced_character_ids(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    signals: &crate::domain::turn::RetrievalSignals,
    provided: &BTreeSet<crate::domain::ids::CharacterId>,
) -> BTreeSet<crate::domain::ids::CharacterId> {
    signals
        .entities
        .iter()
        .filter_map(|signal| match &signal.entity {
            KnowledgeEntity::Character(character_id) => Some(character_id.clone()),
            KnowledgeEntity::Role(role_key) => {
                snapshot.role_binding(role_key).map(|binding| binding.character_id.clone())
            }
            _ => None,
        })
        .filter(|character_id| !provided.contains(character_id))
        .collect()
}

async fn load_relevant_knowledge(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    signals: &crate::domain::turn::RetrievalSignals,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
) -> Result<Vec<RelevantKnowledge>, ContextError> {
    let filter = KnowledgeFilter {
        audience: RetrievalAudience::GlobalWriter,
        knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
        authorized_memory_owners: Vec::new(),
        max_item_bytes: config.max_item_bytes,
    };
    let entities = signals.entities.iter().map(|signal| signal.entity.clone()).collect::<Vec<_>>();
    let topics = signals.topics.iter().map(|signal| signal.topic.clone()).collect::<Vec<_>>();
    let mut hits = Vec::new();
    if !entities.is_empty() {
        hits.extend(
            knowledge
                .find_by_entities(EntityKnowledgeQuery {
                    snapshot: snapshot.knowledge_snapshot(),
                    filter: &filter,
                    entities: &entities,
                    limit: config.max_items_per_audience,
                })
                .await?,
        );
    }
    if !topics.is_empty() && hits.len() < config.max_items_per_audience {
        hits.extend(
            knowledge
                .find_by_topics(TopicKnowledgeQuery {
                    snapshot: snapshot.knowledge_snapshot(),
                    filter: &filter,
                    topics: &topics,
                    limit: config.max_items_per_audience.saturating_sub(hits.len()),
                })
                .await?,
        );
    }
    normalize_relevant_knowledge(hits, signals, config)
}

async fn load_knowledge_index(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    relevant: &[RelevantKnowledge],
    config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
) -> Result<(RetrievalIndexScope, Vec<KnowledgeEntryIndexEntry>), ContextError> {
    let requested = config.max_candidates_total.saturating_add(1);
    let records = knowledge
        .list_index(KnowledgeIndexQuery {
            snapshot: snapshot.knowledge_snapshot(),
            knowledge_kinds: &[KnowledgeKind::Fact, KnowledgeKind::Rumor],
            limit: requested,
        })
        .await?;
    let scope = if records.len() > config.max_candidates_total {
        RetrievalIndexScope::Prefiltered
    } else {
        RetrievalIndexScope::Complete
    };
    let provided = relevant.iter().map(|entry| &entry.entry_id).collect::<BTreeSet<_>>();
    let mut entries = Vec::new();
    for record in records.into_iter().take(config.max_candidates_total) {
        if provided.contains(&record.source_id) {
            continue;
        }
        let hint = match record.kind {
            KnowledgeKind::Fact => "objective fact entry",
            KnowledgeKind::Rumor => "public claim entry",
            KnowledgeKind::Memory => "character memory entry",
        };
        entries.push(KnowledgeEntryIndexEntry {
            target_id: RetrievalTargetId::for_knowledge(&record.source_id),
            entry_id: record.source_id,
            kind: record.kind,
            retrieval_hint: BoundedText::try_new(hint, "retrieval_hint", 128).map_err(|_| {
                ContextError::InvalidRecord {
                    code: "retrieval_hint_invalid",
                }
            })?,
        });
    }
    Ok((scope, entries))
}

fn normalize_relevant_knowledge(
    hits: Vec<KnowledgeLookupHit>,
    signals: &crate::domain::turn::RetrievalSignals,
    config: &RetrievalConfig,
) -> Result<Vec<RelevantKnowledge>, ContextError> {
    let mut by_id = std::collections::BTreeMap::new();
    for hit in hits {
        let priority = hit
            .matches
            .iter()
            .filter_map(|matched| match matched {
                KnowledgeIndexMatch::Entity(entity) => signals
                    .entities
                    .iter()
                    .find(|signal| &signal.entity == entity)
                    .map(|signal| signal.priority),
                KnowledgeIndexMatch::Topic(topic) => signals
                    .topics
                    .iter()
                    .find(|signal| &signal.topic == topic)
                    .map(|signal| signal.priority),
            })
            .min()
            .ok_or(ContextError::InvalidRecord {
                code: "preplanning_match_missing",
            })?;
        let entry = RelevantKnowledge {
            entry_id: hit.record.source_id.clone(),
            kind: hit.record.kind,
            content: hit.record.content,
            source_priority: priority,
            salience: hit.record.salience,
        };
        by_id
            .entry(hit.record.source_id)
            .and_modify(|existing: &mut RelevantKnowledge| {
                existing.source_priority = existing.source_priority.min(priority);
            })
            .or_insert(entry);
    }
    let mut entries = by_id.into_values().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.source_priority
            .cmp(&right.source_priority)
            .then_with(|| right.salience.cmp(&left.salience))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    entries.truncate(config.max_items_per_audience);
    let mut tokens = 0u64;
    entries.retain(|entry| {
        let next = tokens.saturating_add(crate::domain::text::estimate_text_tokens(entry.content.as_str()));
        if next > config.max_tokens_per_audience {
            false
        } else {
            tokens = next;
            true
        }
    });
    Ok(entries)
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
