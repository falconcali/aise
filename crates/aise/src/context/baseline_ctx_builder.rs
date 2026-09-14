use crate::config::{
    ActivationConfig, AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, RetrievalConfig,
    TurnContentLimitsConfig,
};
use crate::context::activation::KnowledgeActivationCoordinator;
use crate::context::error::ContextError;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::knowledge::activation::{
    ActivationMacroValues, ActivationRunMode, ActivationScanBuffer, GenerationTrigger, ScanFragment, ScanFragmentKind,
};
use crate::domain::narrative_graph::projector::{NarrativeProjection, NarrativeProjectionInput, NarrativeProjector};
use crate::domain::narrative_graph::state_view::CommittedNarrativeStateView;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{
    BaselineContext, KnowledgeDelivery, KnowledgeIndexEntry, NarrativeGraphStateIndex, RelevantWorldKnowledge,
    RelevantWorldKnowledgeItem, RoleContextView, RoleIndexEntry, SnapshotLimits,
};
use crate::persistence::knowledge_read_port::{KnowledgeFilter, KnowledgeIndexQuery, SourceKnowledgeQuery};
use crate::persistence::store::Store;
use crate::turn::turn_context::{PreparedActivation, TurnExecutionContext};
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
    narrative_config: NarrativeConfig,
    retrieval_config: RetrievalConfig,
    activation_config: ActivationConfig,
    coordinator: Arc<KnowledgeActivationCoordinator>,
    narrative_projector: NarrativeProjector,
}

impl BaselineContextBuilder {
    pub fn new(
        store: Arc<dyn Store>,
        content_limits: TurnContentLimitsConfig,
        context_config: ContextPreparationConfig,
        asset_limits: AssetLimitsConfig,
        narrative_config: NarrativeConfig,
        retrieval_config: RetrievalConfig,
        activation_config: ActivationConfig,
        coordinator: Arc<KnowledgeActivationCoordinator>,
    ) -> Self {
        let narrative_projector = NarrativeProjector::new(narrative_config.as_limits());
        Self {
            store,
            content_limits,
            context_config,
            asset_limits,
            narrative_config,
            retrieval_config,
            activation_config,
            coordinator,
            narrative_projector,
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
            &self.narrative_config,
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
        let pending = ctx.trace().begin_span("context.prepare", "context.prepare");
        let prepared = prepare_baseline(
            &snapshot,
            ctx.player_contribution(),
            ctx.turn_number(),
            &self.context_config,
            &self.retrieval_config,
            &self.activation_config,
            &self.coordinator,
            &self.narrative_projector,
        )
        .await;
        let payload = match &prepared {
            Ok((baseline, projection, activation)) => serde_json::json!({
                "story_id": story_id,
                "turn_number": ctx.turn_number().get(),
                "base_revision": snapshot.base_revision().get(),
                "relevant_role_count": baseline.relevant_roles.len(),
                "constraint_count": baseline.active_story_constraints.len(),
                "activated_count": activation.continuation.activated.len(),
                "active_node_count": projection.plan.active_nodes.len(),
                "status": "ok",
                "error_code": null,
            }),
            Err(error) => serde_json::json!({
                "story_id": story_id,
                "turn_number": ctx.turn_number().get(),
                "base_revision": snapshot.base_revision().get(),
                "relevant_role_count": 0,
                "constraint_count": 0,
                "activated_count": 0,
                "active_node_count": 0,
                "status": "error",
                "error_code": error.turn_code(),
            }),
        };
        ctx.trace().end_span_with(pending, &payload);
        let (baseline, narrative_projection, activation) = prepared.map_err(map_baseline_error)?;
        ctx.set_prepared_context(snapshot, baseline, narrative_projection, activation)
    }
}

async fn prepare_baseline(
    snapshot: &StoryReadSnapshot,
    player_contribution: &str,
    turn_number: crate::domain::ids::TurnNumber,
    context_config: &ContextPreparationConfig,
    retrieval_config: &RetrievalConfig,
    activation_config: &ActivationConfig,
    coordinator: &KnowledgeActivationCoordinator,
    narrative_projector: &NarrativeProjector,
) -> Result<(BaselineContext, NarrativeProjection, PreparedActivation), ContextError> {
    let player_role_view = snapshot
        .role(snapshot.player_role_id())
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_player_role",
        })?;
    let player_role = project_role_context(player_role_view);
    let committed_view = CommittedNarrativeStateView::new(snapshot);
    let current_turn = snapshot.base_revision().get().saturating_add(1);
    let narrative_projection = narrative_projector
        .project(NarrativeProjectionInput {
            definition: snapshot.narrative_definition(),
            state: snapshot.narrative_state(),
            committed_view: &committed_view,
            current_turn,
        })
        .map_err(|_| ContextError::SnapshotInconsistent {
            code: "narrative_projection_failed",
        })?;
    let scan_buffer = build_activation_scan_buffer(
        snapshot,
        &player_role,
        player_contribution,
        &narrative_projection,
        activation_config,
    )?;
    let macros = ActivationMacroValues {
        player_name: player_role.profile.name.as_str().to_owned(),
        player_role_label: player_role.role_label.as_str().to_owned(),
    };
    let activation_result = coordinator
        .run(
            snapshot.knowledge_snapshot(),
            &scan_buffer,
            macros,
            snapshot.story_id(),
            turn_number,
            GenerationTrigger::Normal,
            ActivationRunMode::CommitEligible,
            &[],
            None,
        )
        .await?;
    let relevant_world_knowledge =
        load_relevant_knowledge(snapshot, &activation_result, retrieval_config, coordinator.knowledge()).await?;
    let knowledge_index =
        load_knowledge_index(snapshot, &relevant_world_knowledge, retrieval_config, coordinator.knowledge()).await?;
    let relevant_roles = select_relevant_roles(snapshot, context_config.max_relevant_roles);
    let selected: BTreeSet<RoleId> = std::iter::once(player_role.role_id.clone())
        .chain(relevant_roles.iter().map(|role| role.role_id.clone()))
        .collect();
    let mut role_index = Vec::new();
    for (role_id, role) in snapshot.roles() {
        if selected.contains(role_id) {
            continue;
        }
        role_index.push(RoleIndexEntry {
            role_id: role_id.clone(),
            retrieval_hint: role.narrative_function.clone(),
        });
    }
    role_index.sort_by(|left, right| left.role_id.cmp(&right.role_id));
    if role_index.len() > context_config.max_role_index {
        return Err(ContextError::IndexLimitExceeded {
            index: "role_index",
            actual: role_index.len(),
            maximum: context_config.max_role_index,
        });
    }
    let baseline = BaselineContext {
        story_title: snapshot.story_title().clone(),
        story_profile: snapshot.story_profile().clone(),
        instance_settings: snapshot.instance_settings().clone(),
        player_role,
        relevant_roles,
        relevant_world_knowledge,
        role_index,
        knowledge_index,
        story_continuity: snapshot.story_continuity().clone(),
        active_story_constraints: snapshot.active_constraints().to_vec(),
        narrative_graph_state_index: NarrativeGraphStateIndex {
            pack_digest: snapshot.pack().digest.clone(),
            graph_revision: snapshot.graph_revision(),
            node_states: snapshot.narrative_state().node_states.clone(),
        },
    };
    let activation = PreparedActivation {
        continuation: activation_result.continuation,
        pending_timed_state: activation_result.pending_timed_state,
    };
    Ok((baseline, narrative_projection, activation))
}

pub(crate) fn build_activation_scan_buffer(
    snapshot: &StoryReadSnapshot,
    player_role: &RoleContextView,
    player_contribution: &str,
    narrative_projection: &NarrativeProjection,
    activation_config: &ActivationConfig,
) -> Result<ActivationScanBuffer, ContextError> {
    let max_item_bytes = activation_config.runtime.max_single_entry_bytes;
    let mut fragments = Vec::new();
    let contribution = BoundedText::try_new(player_contribution.to_owned(), "player_contribution", max_item_bytes)
        .map_err(|_| ContextError::InvalidRecord {
            code: "scan_player_contribution",
        })?;
    fragments.push(ScanFragment::new(ScanFragmentKind::PlayerContribution, 0, 0, contribution));
    if !player_role.profile.name.as_str().is_empty() {
        fragments.push(ScanFragment::new(
            ScanFragmentKind::PlayerRoleName,
            0,
            0,
            player_role.profile.name.clone(),
        ));
    }
    if !player_role.role_label.as_str().is_empty() {
        fragments.push(ScanFragment::new(
            ScanFragmentKind::PlayerRoleLabel,
            0,
            0,
            player_role.role_label.clone(),
        ));
    }
    for (order, direction) in narrative_projection.plan.active_directions.iter().enumerate() {
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::NarrativeDirection,
            0,
            stable_order,
            direction.dramatic_focus.clone(),
        ));
    }
    for (order, event) in narrative_projection.plan.world_event_intents.iter().enumerate() {
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::NarrativeEvent,
            0,
            stable_order,
            event.description.clone(),
        ));
    }
    let recent = snapshot.story_continuity().recent_segments();
    for (order, segment) in recent.iter().rev().enumerate() {
        let depth = u16::try_from(order.saturating_add(1)).unwrap_or(u16::MAX);
        let stable_order = u32::try_from(order).unwrap_or(u32::MAX);
        fragments.push(ScanFragment::new(
            ScanFragmentKind::RecentStory,
            depth,
            stable_order,
            segment.text.clone(),
        ));
    }
    let summary = snapshot.story_continuity().summary();
    if !summary.text.as_str().is_empty() {
        let summary_depth = if activation_config.runtime.include_summary_at_max_depth {
            activation_config.runtime.max_scan_depth
        } else {
            u16::try_from(recent.len().saturating_add(1)).unwrap_or(u16::MAX)
        };
        fragments.push(ScanFragment::new(
            ScanFragmentKind::StorySummary,
            summary_depth,
            0,
            summary.text.clone(),
        ));
    }
    ActivationScanBuffer::try_new(
        fragments,
        activation_config.runtime.max_scan_fragments,
        activation_config.runtime.max_scan_bytes,
    )
    .map_err(|_| ContextError::InvalidRecord {
        code: "activation_scan_buffer",
    })
}

fn project_role_context(role: &crate::domain::story_instance::role::StoryRoleView) -> RoleContextView {
    RoleContextView::from(role)
}

fn select_relevant_roles(snapshot: &StoryReadSnapshot, max_relevant_roles: usize) -> Vec<RoleContextView> {
    let player_role_id = snapshot.player_role_id();
    let Some(player) = snapshot.role(player_role_id) else {
        return Vec::new();
    };
    let location = &player.state.location;
    let mut roles = snapshot
        .roles()
        .iter()
        .filter(|(role_id, role)| *role_id != player_role_id && &role.state.location == location)
        .map(|(_, role)| project_role_context(role))
        .collect::<Vec<_>>();
    roles.sort_by(|left, right| left.role_id.cmp(&right.role_id));
    roles.truncate(max_relevant_roles);
    roles
}

async fn load_relevant_knowledge(
    snapshot: &StoryReadSnapshot,
    activation: &crate::domain::knowledge::activation::ActivationResult,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn crate::persistence::knowledge_read_port::KnowledgeReadPort>,
) -> Result<RelevantWorldKnowledge, ContextError> {
    let admitted = activation
        .activated
        .iter()
        .filter(|entry| {
            entry.deliveries.contains(&KnowledgeDelivery::Writer)
                && matches!(entry.source_id.kind(), KnowledgeKind::Fact | KnowledgeKind::Rumor)
        })
        .cloned()
        .collect::<Vec<_>>();
    if admitted.is_empty() {
        return Ok(RelevantWorldKnowledge::default());
    }
    let source_ids = admitted.iter().map(|entry| entry.source_id.clone()).collect::<Vec<_>>();
    let filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
        max_item_bytes: config.max_item_bytes,
    };
    let records = knowledge
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: snapshot.knowledge_snapshot(),
            filter: &filter,
            source_ids: &source_ids,
            limit: source_ids.len().min(config.max_items_per_audience),
        })
        .await?;
    let mut by_id = records
        .into_iter()
        .map(|record| (record.source_id.clone(), record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut entries = Vec::new();
    for activated in admitted {
        let Some(record) = by_id.remove(&activated.source_id) else {
            continue;
        };
        let source_priority = u8::try_from(activated.rank.min(u32::from(u8::MAX))).unwrap_or(u8::MAX);
        entries.push(RelevantWorldKnowledgeItem {
            source_id: record.source_id,
            content: record.content,
            source_priority,
            salience: record.salience,
        });
    }
    entries.sort_by(|left, right| {
        left.source_priority
            .cmp(&right.source_priority)
            .then_with(|| right.salience.cmp(&left.salience))
            .then_with(|| left.source_id.cmp(&right.source_id))
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
    let mut result = RelevantWorldKnowledge::default();
    for entry in entries {
        match entry.source_id.kind() {
            KnowledgeKind::Fact => result.facts.push(entry),
            KnowledgeKind::Rumor => result.rumors.push(entry),
            KnowledgeKind::Memory => {}
        }
    }
    Ok(result)
}

async fn load_knowledge_index(
    snapshot: &StoryReadSnapshot,
    relevant: &RelevantWorldKnowledge,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn crate::persistence::knowledge_read_port::KnowledgeReadPort>,
) -> Result<Vec<KnowledgeIndexEntry>, ContextError> {
    let requested = config.max_candidates_total.saturating_add(1);
    let records = knowledge
        .list_index(KnowledgeIndexQuery {
            snapshot: snapshot.knowledge_snapshot(),
            knowledge_kinds: &[KnowledgeKind::Fact, KnowledgeKind::Rumor],
            limit: requested,
        })
        .await?;
    if records.len() > config.max_candidates_total {
        return Err(ContextError::IndexLimitExceeded {
            index: "knowledge_index",
            actual: records.len(),
            maximum: config.max_candidates_total,
        });
    }
    let provided = relevant
        .facts
        .iter()
        .chain(relevant.rumors.iter())
        .map(|entry| &entry.source_id)
        .collect::<BTreeSet<_>>();
    let mut entries = Vec::new();
    for record in records {
        if provided.contains(&record.source_id) {
            continue;
        }
        entries.push(KnowledgeIndexEntry {
            source_id: record.source_id,
            retrieval_hint: record.retrieval_hint,
        });
    }
    Ok(entries)
}

fn map_baseline_error(error: ContextError) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        error.turn_code(),
        Some(TurnStage::BaselineBuilder),
        error.to_string(),
    )
}

#[cfg(test)]
#[path = "tests/baseline_ctx_builder_tests.rs"]
mod tests;
