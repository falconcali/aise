use crate::config::{AssetLimitsConfig, ContextPreparationConfig, RetrievalConfig, TurnContentLimitsConfig};
use crate::context::error::ContextError;
use crate::context::retrieval_signal_builder::RetrievalSignalBuilder;
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeIndexMatch, KnowledgeKind};
use crate::domain::narrative::StoryContinuityLimits;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{
    BaselineContext, KnowledgeDelivery, KnowledgeIndexEntry, NarrativeGraphStateIndex, RelevantWorldKnowledge,
    RelevantWorldKnowledgeItem, RetrievalSignals, RoleContextView, RoleIndexEntry, SnapshotLimits,
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
use std::collections::{BTreeMap, BTreeSet};
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
            ctx.player_contribution(),
            &self.signal_builder,
            &self.context_config,
            &self.retrieval_config,
            &self.knowledge,
        )
        .await;
        let payload = match &baseline_result {
            Ok(baseline) => serde_json::json!({
                "story_id": story_id,
                "turn_number": ctx.turn_number().get(),
                "base_revision": snapshot.base_revision().get(),
                "relevant_role_count": baseline.relevant_roles.len(),
                "constraint_count": baseline.active_story_constraints.len(),
                "entity_signal_count": baseline.retrieval_signals.entities.len(),
                "topic_signal_count": baseline.retrieval_signals.topics.len(),
                "status": "ok",
                "error_code": null,
            }),
            Err(error) => serde_json::json!({
                "story_id": story_id,
                "turn_number": ctx.turn_number().get(),
                "base_revision": snapshot.base_revision().get(),
                "relevant_role_count": 0,
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
    snapshot: &StoryReadSnapshot,
    player_contribution: &str,
    signal_builder: &RetrievalSignalBuilder,
    context_config: &ContextPreparationConfig,
    retrieval_config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
) -> Result<BaselineContext, ContextError> {
    let player_role_view = snapshot
        .role(snapshot.player_role_id())
        .ok_or(ContextError::SnapshotInconsistent {
            code: "missing_player_role",
        })?;
    let player_role = project_role_context(player_role_view);
    let retrieval_signals = signal_builder.build(snapshot, player_contribution)?;
    let relevant_roles = select_relevant_roles(snapshot, &retrieval_signals, context_config.max_relevant_roles);
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
    let relevant_world_knowledge =
        load_relevant_knowledge(snapshot, &retrieval_signals, retrieval_config, knowledge).await?;
    let knowledge_index =
        load_knowledge_index(snapshot, &relevant_world_knowledge, retrieval_config, knowledge).await?;
    Ok(BaselineContext {
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
        retrieval_signals,
    })
}

fn project_role_context(role: &crate::domain::story_instance::role::StoryRoleView) -> RoleContextView {
    RoleContextView::from(role)
}

fn select_relevant_roles(
    snapshot: &StoryReadSnapshot,
    signals: &RetrievalSignals,
    max_relevant_roles: usize,
) -> Vec<RoleContextView> {
    let player_role_id = snapshot.player_role_id();
    let mut best_priority: BTreeMap<RoleId, u8> = BTreeMap::new();
    for signal in &signals.entities {
        if let KnowledgeEntity::Role(role_id) = &signal.entity {
            if role_id == player_role_id {
                continue;
            }
            best_priority
                .entry(role_id.clone())
                .and_modify(|priority| *priority = (*priority).min(signal.priority))
                .or_insert(signal.priority);
        }
    }
    let mut ranked: Vec<(u8, RoleId)> = best_priority
        .into_iter()
        .map(|(role_id, priority)| (priority, role_id))
        .collect();
    ranked.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    ranked
        .into_iter()
        .take(max_relevant_roles)
        .filter_map(|(_, role_id)| snapshot.role(&role_id).map(project_role_context))
        .collect()
}

async fn load_relevant_knowledge(
    snapshot: &StoryReadSnapshot,
    signals: &crate::domain::turn::RetrievalSignals,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
) -> Result<RelevantWorldKnowledge, ContextError> {
    let filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
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
    snapshot: &StoryReadSnapshot,
    relevant: &RelevantWorldKnowledge,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn KnowledgeReadPort>,
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

fn normalize_relevant_knowledge(
    hits: Vec<KnowledgeLookupHit>,
    signals: &crate::domain::turn::RetrievalSignals,
    config: &RetrievalConfig,
) -> Result<RelevantWorldKnowledge, ContextError> {
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
        let entry = RelevantWorldKnowledgeItem {
            source_id: hit.record.source_id.clone(),
            content: hit.record.content,
            source_priority: priority,
            salience: hit.record.salience,
        };
        by_id
            .entry(hit.record.source_id)
            .and_modify(|existing: &mut RelevantWorldKnowledgeItem| {
                existing.source_priority = existing.source_priority.min(priority);
            })
            .or_insert(entry);
    }
    let mut entries = by_id.into_values().collect::<Vec<_>>();
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
