use crate::config::{ActivationConfig, RetrievalConfig};
use crate::context::activation::KnowledgeActivationCoordinator;
use crate::context::baseline_ctx_builder::build_activation_scan_buffer;
use crate::context::error::ContextError;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::activation::{
    ActivationMacroValues, ActivationRunMode, ActivationSeedKind, ExternalActivationSeed, GenerationTrigger,
};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::turn::{
    KnowledgeDelivery, RetrievedCharacterContext, RetrievedContext, RetrievedContextError, RetrievedContextLimits,
    RetrievedKnowledgeItem, RetrievedWorldKnowledge, RoleContextView,
};
use crate::persistence::knowledge_read_port::{KnowledgeFilter, OwnerMemoryQuery, SourceKnowledgeQuery};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub struct ContextRetrievalPipeline {
    config: RetrievalConfig,
    activation_config: ActivationConfig,
    coordinator: Arc<KnowledgeActivationCoordinator>,
}

impl ContextRetrievalPipeline {
    pub fn new(
        config: RetrievalConfig,
        activation_config: ActivationConfig,
        coordinator: Arc<KnowledgeActivationCoordinator>,
    ) -> Self {
        Self {
            config,
            activation_config,
            coordinator,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::ContextRetrieval
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let plan = ctx
            .plan()
            .ok_or_else(|| map_context_error(ContextError::InvalidPlan { code: "missing_plan" }))?
            .clone();
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| {
                map_context_error(ContextError::SnapshotInconsistent {
                    code: "missing_snapshot",
                })
            })?
            .clone();
        let baseline = ctx
            .baseline()
            .ok_or_else(|| {
                map_context_error(ContextError::SnapshotInconsistent {
                    code: "missing_baseline",
                })
            })?
            .clone();
        let narrative_projection = ctx
            .narrative_projection()
            .ok_or_else(|| {
                map_context_error(ContextError::SnapshotInconsistent {
                    code: "missing_narrative_projection",
                })
            })?
            .clone();
        let prepared = ctx
            .activation()
            .ok_or_else(|| {
                map_context_error(ContextError::SnapshotInconsistent {
                    code: "missing_activation",
                })
            })?
            .clone();
        let pending = ctx.trace().begin_span("context.retrieve", "context.retrieve");
        let mut role_views: BTreeMap<RoleId, RoleContextView> = BTreeMap::new();
        for request in &plan.retrieval_plan.character_requests {
            let role = snapshot.role(&request.role_id).ok_or_else(|| {
                map_context_error(ContextError::SnapshotInconsistent {
                    code: "unknown_character_request_role",
                })
            })?;
            role_views.insert(request.role_id.clone(), RoleContextView::from(role));
        }
        let scan_buffer = build_activation_scan_buffer(
            &snapshot,
            &baseline.player_role,
            ctx.player_contribution(),
            &narrative_projection,
            &self.activation_config,
        )
        .map_err(map_context_error)?;
        let macros = ActivationMacroValues {
            player_name: baseline.player_role.profile.name.as_str().to_owned(),
            player_role_label: baseline.player_role.role_label.as_str().to_owned(),
        };
        let index = self
            .coordinator
            .prepare_index(snapshot.knowledge_snapshot(), macros.clone())
            .await
            .map_err(|error| map_context_error(ContextError::from(error)))?;
        let mut seeds = Vec::new();
        let mut memory_targets: BTreeMap<RoleId, Vec<KnowledgeSourceId>> = BTreeMap::new();
        for request in &plan.retrieval_plan.knowledge_requests {
            match request.target_source_id.kind() {
                KnowledgeKind::Memory => match &request.delivery {
                    KnowledgeDelivery::Character { role_id } => {
                        memory_targets
                            .entry(role_id.clone())
                            .or_default()
                            .push(request.target_source_id.clone());
                    }
                    KnowledgeDelivery::Writer => {
                        return Err(map_context_error(ContextError::KnowledgeAudienceViolation));
                    }
                },
                KnowledgeKind::Fact | KnowledgeKind::Rumor => {
                    if matches!(
                        (&request.delivery, request.target_source_id.kind()),
                        (KnowledgeDelivery::Character { .. }, KnowledgeKind::Fact)
                    ) {
                        return Err(map_context_error(ContextError::KnowledgeAudienceViolation));
                    }
                    if !self
                        .coordinator
                        .authorize_seed(&index, &request.target_source_id, &request.delivery)
                    {
                        return Err(map_context_error(ContextError::InvalidPlan {
                            code: "unauthorized_activation_target",
                        }));
                    }
                    seeds.push(ExternalActivationSeed {
                        source_id: request.target_source_id.clone(),
                        delivery: request.delivery.clone(),
                        kind: ActivationSeedKind::PlannerExactTarget,
                        provider_rank: None,
                        mandatory: request.mandatory,
                    });
                }
            }
        }
        if seeds.len() > self.activation_config.runtime.max_external_candidates {
            return Err(map_context_error(ContextError::CandidateLimitExceeded));
        }
        let activation_result = self
            .coordinator
            .run(
                snapshot.knowledge_snapshot(),
                &scan_buffer,
                macros,
                ctx.story_id(),
                ctx.turn_number(),
                GenerationTrigger::Normal,
                ActivationRunMode::CommitEligible,
                &seeds,
                Some(prepared.continuation.clone()),
            )
            .await
            .map_err(|error| map_context_error(ContextError::from(error)))?;
        let baseline_ids = baseline
            .relevant_world_knowledge
            .facts
            .iter()
            .chain(baseline.relevant_world_knowledge.rumors.iter())
            .map(|entry| entry.source_id.clone())
            .collect::<BTreeSet<_>>();
        let newly_writer = activation_result
            .activated
            .iter()
            .filter(|entry| {
                entry.deliveries.contains(&KnowledgeDelivery::Writer)
                    && matches!(entry.source_id.kind(), KnowledgeKind::Fact | KnowledgeKind::Rumor)
                    && !baseline_ids.contains(&entry.source_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let writer_items = load_activated_items(
            &snapshot,
            &newly_writer,
            KnowledgeDelivery::Writer,
            &self.config,
            self.coordinator.knowledge(),
        )
        .await
        .map_err(map_context_error)?;
        let mut world = RetrievedWorldKnowledge::default();
        for item in writer_items {
            match item.source_id.kind() {
                KnowledgeKind::Fact => world.facts.push(item),
                KnowledgeKind::Rumor => world.rumors.push(item),
                KnowledgeKind::Memory => {}
            }
        }
        let mut characters: BTreeMap<RoleId, RetrievedCharacterContext> = BTreeMap::new();
        for (role_id, role_view) in &role_views {
            let rumor_refs = activation_result
                .activated
                .iter()
                .filter(|entry| {
                    entry.source_id.kind() == KnowledgeKind::Rumor
                        && (entry.deliveries.contains(&KnowledgeDelivery::Writer)
                            || entry.deliveries.contains(&KnowledgeDelivery::Character {
                                role_id: role_id.clone(),
                            }))
                })
                .cloned()
                .collect::<Vec<_>>();
            let known_rumors = load_activated_items(
                &snapshot,
                &rumor_refs,
                KnowledgeDelivery::Character {
                    role_id: role_id.clone(),
                },
                &self.config,
                self.coordinator.knowledge(),
            )
            .await
            .map_err(map_context_error)?;
            let memories = if let Some(targets) = memory_targets.get(role_id) {
                let filter = KnowledgeFilter {
                    delivery: KnowledgeDelivery::Character {
                        role_id: role_id.clone(),
                    },
                    knowledge_kinds: vec![KnowledgeKind::Memory],
                    max_item_bytes: self.config.max_item_bytes,
                };
                let records = self
                    .coordinator
                    .knowledge()
                    .find_by_source_ids(SourceKnowledgeQuery {
                        snapshot: snapshot.knowledge_snapshot(),
                        filter: &filter,
                        source_ids: targets,
                        limit: targets.len(),
                    })
                    .await
                    .map_err(|error| map_context_error(ContextError::from(error)))?;
                for record in &records {
                    if record.memory_owner.as_ref() != Some(role_id) {
                        return Err(map_retrieved_context_error(RetrievedContextError::InvalidMemoryOwner));
                    }
                }
                records
            } else {
                self.coordinator
                    .knowledge()
                    .find_memories_by_owner(OwnerMemoryQuery {
                        snapshot: snapshot.knowledge_snapshot(),
                        owner: role_id,
                        limit: self.config.max_items_per_audience,
                        max_item_bytes: self.config.max_item_bytes,
                    })
                    .await
                    .map_err(|error| map_context_error(ContextError::from(error)))?
            };
            let mut character = RetrievedCharacterContext {
                role: Some(role_view.clone()),
                known_rumors,
                memories: memories
                    .into_iter()
                    .enumerate()
                    .map(|(index, record)| {
                        let rank = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
                        RetrievedKnowledgeItem::from_parts(
                            record.source_id,
                            record.content,
                            record.source,
                            ActivationSeedKind::PlannerExactTarget,
                            rank,
                            record.salience,
                        )
                    })
                    .collect(),
            };
            sort_partition(&mut character.known_rumors);
            sort_partition(&mut character.memories);
            character.known_rumors.truncate(self.config.max_items_per_audience);
            character.memories.truncate(self.config.max_items_per_audience);
            trim_tokens(&mut character.known_rumors, self.config.max_tokens_per_audience);
            trim_tokens(&mut character.memories, self.config.max_tokens_per_audience);
            characters.insert(role_id.clone(), character);
        }
        sort_partition(&mut world.facts);
        sort_partition(&mut world.rumors);
        world.facts.truncate(self.config.max_items_per_audience);
        world.rumors.truncate(self.config.max_items_per_audience);
        trim_tokens(&mut world.facts, self.config.max_tokens_per_audience);
        trim_tokens(&mut world.rumors, self.config.max_tokens_per_audience);
        let limits = RetrievedContextLimits {
            max_role_audiences: ctx.budget().max_character_decisions(),
            max_items_per_audience: self.config.max_items_per_audience,
            max_tokens_per_audience: self.config.max_tokens_per_audience,
            max_total_items: self.config.max_total_items,
            max_total_tokens: self.config.max_total_tokens,
            max_item_bytes: self.config.max_item_bytes,
        };
        let context = RetrievedContext::try_new(world, characters, limits).map_err(map_retrieved_context_error)?;
        let payload = serde_json::json!({
            "story_id": ctx.story_id(),
            "turn_number": ctx.turn_number().get(),
            "base_revision": snapshot.base_revision().get(),
            "character_request_count": plan.retrieval_plan.character_requests.len(),
            "knowledge_request_count": plan.retrieval_plan.knowledge_requests.len(),
            "world_item_count": context.world().facts.len() + context.world().rumors.len(),
            "character_partition_count": context.characters().len(),
            "total_tokens": context.total_tokens(),
            "status": "ok",
            "error_code": null,
        });
        ctx.trace().end_span_with(pending, &payload);
        ctx.replace_activation(activation_result);
        ctx.set_retrieved_context(context)
    }
}

async fn load_activated_items(
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
    activated: &[crate::domain::knowledge::activation::ActivatedKnowledgeRef],
    delivery: KnowledgeDelivery,
    config: &RetrievalConfig,
    knowledge: &Arc<dyn crate::persistence::knowledge_read_port::KnowledgeReadPort>,
) -> Result<Vec<RetrievedKnowledgeItem>, ContextError> {
    if activated.is_empty() {
        return Ok(Vec::new());
    }
    let source_ids = activated.iter().map(|entry| entry.source_id.clone()).collect::<Vec<_>>();
    let kinds = match delivery {
        KnowledgeDelivery::Writer => vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
        KnowledgeDelivery::Character { .. } => vec![KnowledgeKind::Rumor],
    };
    let filter = KnowledgeFilter {
        delivery,
        knowledge_kinds: kinds,
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
    let by_id = records
        .into_iter()
        .map(|record| (record.source_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut items = Vec::new();
    for entry in activated {
        let Some(record) = by_id.get(&entry.source_id) else {
            continue;
        };
        let mut item = RetrievedKnowledgeItem::from_parts(
            record.source_id.clone(),
            record.content.clone(),
            record.source.clone(),
            entry.activation_class,
            entry.rank,
            record.salience,
        );
        if let (Some(activation), Some(version)) = (&record.activation, &record.activation_rule_version) {
            item = item.with_activation(activation.clone(), version.clone());
        }
        items.push(item);
    }
    Ok(items)
}

fn sort_partition(items: &mut [RetrievedKnowledgeItem]) {
    items.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| right.salience.cmp(&left.salience))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
}

fn trim_tokens(items: &mut Vec<RetrievedKnowledgeItem>, max_tokens: u64) {
    let mut total = 0u64;
    let mut keep = 0usize;
    for item in items.iter() {
        let next = total.saturating_add(item.token_cost);
        if next > max_tokens {
            break;
        }
        total = next;
        keep = keep.saturating_add(1);
    }
    items.truncate(keep);
}

fn map_context_error(error: ContextError) -> TurnExecutionError {
    let stage = match &error {
        ContextError::SnapshotInconsistent { .. }
        | ContextError::ContinuityInvalid { .. }
        | ContextError::SignalLimitExceeded { .. } => Some(TurnStage::BaselineBuilder),
        _ => Some(TurnStage::ContextRetrieval),
    };
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, error.turn_code(), stage, error.to_string())
}

fn map_retrieved_context_error(error: RetrievedContextError) -> TurnExecutionError {
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        error.turn_code(),
        Some(TurnStage::ContextRetrieval),
        error.to_string(),
    )
}

#[cfg(test)]
#[path = "tests/retrieval_pipeline_tests.rs"]
mod tests;
