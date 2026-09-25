use crate::config::{
    ActivationConfig, AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, RetrievalConfig,
    TurnContentLimitsConfig,
};
use crate::context::activation::KnowledgeActivationCoordinator;
use crate::context::activation::scan_builder::build_activation_scan_buffer;
use crate::context::error::ContextError;
use crate::context::observability;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::knowledge::activation::{
    ActivationMacroValues, ActivationRunMode, GenerationTrigger, LoadedActivationEntry,
};
use crate::domain::narrative_graph::projector::{NarrativeProjection, NarrativeProjectionInput, NarrativeProjector};
use crate::domain::narrative_graph::state_view::CommittedNarrativeStateView;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{
    BaselineContext, KnowledgeDelivery, KnowledgeIndexEntry, NarrativeGraphStateIndex, RelevantWorldKnowledge,
    RelevantWorldKnowledgeItem, RoleContextView, RoleIndexEntry, SnapshotLimits,
};
use crate::persistence::knowledge_read_port::KnowledgeIndexQuery;
use crate::persistence::store::Store;
use crate::turn::turn_context::{PreparedActivation, TurnExecutionContext};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub struct BaselineContextBuilderConfig {
    pub content_limits: TurnContentLimitsConfig,
    pub context_config: ContextPreparationConfig,
    pub asset_limits: AssetLimitsConfig,
    pub narrative_config: NarrativeConfig,
    pub retrieval_config: RetrievalConfig,
    pub activation_config: ActivationConfig,
}

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
        config: BaselineContextBuilderConfig,
        coordinator: Arc<KnowledgeActivationCoordinator>,
    ) -> Self {
        let narrative_projector =
            NarrativeProjector::new(crate::turn::turn_budget::narrative_limits(&config.narrative_config));
        Self {
            store,
            content_limits: config.content_limits,
            context_config: config.context_config,
            asset_limits: config.asset_limits,
            narrative_config: config.narrative_config,
            retrieval_config: config.retrieval_config,
            activation_config: config.activation_config,
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

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
        observation: &crate::observability::Observation,
    ) -> Result<(), TurnExecutionError> {
        let story_id = ctx.story_id().clone();
        let limits = SnapshotLimits::from_config(
            &self.content_limits,
            &self.context_config,
            &self.asset_limits,
            &self.narrative_config,
        );
        let snapshot_observation = observability::begin_load_story_snapshot_observation(observation, ctx);
        let outcome = self.store.load_story_snapshot(&story_id, limits).await;
        observability::finish_observation(snapshot_observation, &outcome);
        let snapshot = outcome.map_err(TurnExecutionError::from)?;

        let activation_observation = observability::begin_activate_world_info_observation(observation, ctx);
        let prepared = prepare_baseline(self, &snapshot, ctx.player_contribution(), ctx.turn_number()).await;
        observability::finish_observation(activation_observation, &prepared);

        let (baseline, narrative_projection, activation) = prepared.map_err(map_baseline_error)?;
        ctx.set_prepared_context(snapshot, baseline, narrative_projection, activation)
    }
}

async fn prepare_baseline(
    builder: &BaselineContextBuilder,
    snapshot: &StoryReadSnapshot,
    player_contribution: &str,
    turn_number: crate::domain::ids::TurnNumber,
) -> Result<(BaselineContext, NarrativeProjection, PreparedActivation), ContextError> {
    let player_role_view = snapshot
        .role(snapshot.player_role_id())
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_player_role",
        })?;
    let player_role = project_role_context(player_role_view);
    let committed_view = CommittedNarrativeStateView::new(snapshot);
    let current_turn = snapshot.base_revision().get().saturating_add(1);
    let narrative_projection = builder
        .narrative_projector
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
        &builder.activation_config,
    )?;
    let macros = ActivationMacroValues {
        player_name: player_role.profile.name.as_str().to_owned(),
        player_role_label: player_role.role_label.as_str().to_owned(),
    };
    let activation_outcome = builder
        .coordinator
        .run(crate::context::activation::ActivationRunSpec {
            snapshot: snapshot.knowledge_snapshot(),
            scan_buffer: &scan_buffer,
            macros,
            story_id: snapshot.story_id(),
            turn_number,
            generation_trigger: GenerationTrigger::Normal,
            mode: ActivationRunMode::CommitEligible,
            external_seeds: &[],
            continuation: None,
        })
        .await?;
    let relevant_world_knowledge = load_relevant_knowledge(
        &activation_outcome.result,
        &activation_outcome.loaded_entries,
        &builder.retrieval_config,
    )?;
    let knowledge_index = load_knowledge_index(
        snapshot,
        &relevant_world_knowledge,
        &builder.retrieval_config,
        builder.coordinator.knowledge(),
    )
    .await?;
    let relevant_roles = select_relevant_roles(snapshot, builder.context_config.max_relevant_roles);
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
    if role_index.len() > builder.context_config.max_role_index {
        return Err(ContextError::IndexLimitExceeded {
            index: "role_index",
            actual: role_index.len(),
            maximum: builder.context_config.max_role_index,
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
        continuation: activation_outcome.result.continuation,
        pending_timed_state: activation_outcome.result.pending_timed_state,
        index_snapshot: activation_outcome.index_snapshot,
        loaded_entries: activation_outcome.loaded_entries,
    };
    Ok((baseline, narrative_projection, activation))
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

fn load_relevant_knowledge(
    activation: &crate::domain::knowledge::activation::ActivationResult,
    loaded_entries: &BTreeMap<crate::domain::knowledge::KnowledgeSourceId, LoadedActivationEntry>,
    config: &RetrievalConfig,
) -> Result<RelevantWorldKnowledge, ContextError> {
    let mut entries = Vec::new();
    for activated in activation.activated.iter().filter(|entry| {
        entry.deliveries.contains(&KnowledgeDelivery::Writer)
            && matches!(entry.source_id.kind(), KnowledgeKind::Fact | KnowledgeKind::Rumor)
    }) {
        let loaded = loaded_entries
            .get(&activated.source_id)
            .ok_or(ContextError::SnapshotInconsistent {
                code: "activation_body_missing",
            })?;
        let source_priority = u8::try_from(activated.rank.min(u32::from(u8::MAX))).unwrap_or(u8::MAX);
        entries.push(RelevantWorldKnowledgeItem {
            source_id: loaded.body.source_id.clone(),
            content: loaded.body.body.clone(),
            source_priority,
            salience: loaded.salience,
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
