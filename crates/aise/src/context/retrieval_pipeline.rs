use crate::config::RetrievalConfig;
use crate::context::candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
use crate::context::error::ContextError;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::turn::{
    CandidateMatch, CandidateRetrieverKind, KnowledgeDelivery, MatchLevel, RelevanceRank, RetrievedCharacterContext,
    RetrievedContext, RetrievedContextError, RetrievedContextLimits, RetrievedKnowledgeItem, RetrievedWorldKnowledge,
    RoleContextView,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ContextRetrievalPipeline {
    config: RetrievalConfig,
    retrievers: Vec<Arc<dyn CandidateRetriever>>,
}

impl ContextRetrievalPipeline {
    pub fn new(config: RetrievalConfig, retrievers: Vec<Arc<dyn CandidateRetriever>>) -> Result<Self, ContextError> {
        if retrievers.is_empty() {
            return Err(ContextError::InvalidRetrieverSet { code: "empty" });
        }
        if retrievers.len() > config.max_candidate_retrievers {
            return Err(ContextError::InvalidRetrieverSet {
                code: "too_many_retrievers",
            });
        }
        let mut seen = BTreeMap::new();
        for retriever in &retrievers {
            if seen.insert(retriever.kind(), ()).is_some() {
                return Err(ContextError::InvalidRetrieverSet { code: "duplicate_kind" });
            }
        }
        if !seen.contains_key(&CandidateRetrieverKind::Entity) || !seen.contains_key(&CandidateRetrieverKind::Topic) {
            return Err(ContextError::InvalidRetrieverSet {
                code: "missing_entity_or_topic",
            });
        }
        Ok(Self { config, retrievers })
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
        let mut candidates = Vec::new();
        let mut total_collected = 0usize;
        let mut fact_candidate_count = 0usize;
        let mut rumor_candidate_count = 0usize;
        let mut memory_candidate_count = 0usize;
        for request in &plan.retrieval_plan.knowledge_requests {
            for retriever in &self.retrievers {
                if total_collected >= self.config.max_candidates_total {
                    break;
                }
                let remaining_total = self.config.max_candidates_total.saturating_sub(total_collected);
                let limit = self
                    .config
                    .max_candidates_per_retriever
                    .min(remaining_total)
                    .min(ctx.budget().max_candidates_per_retriever());
                let batch = retriever
                    .retrieve(CandidateRetrievalRequest {
                        snapshot: snapshot.knowledge_snapshot(),
                        request,
                        limit,
                        max_item_bytes: self.config.max_item_bytes,
                    })
                    .await
                    .map_err(map_context_error)?;
                total_collected = total_collected.saturating_add(batch.len());
                for candidate in &batch {
                    match candidate.record.kind {
                        KnowledgeKind::Fact => fact_candidate_count += 1,
                        KnowledgeKind::Rumor => rumor_candidate_count += 1,
                        KnowledgeKind::Memory => memory_candidate_count += 1,
                    }
                }
                candidates.extend(batch);
            }
        }
        let merged = merge_candidates(candidates)?;
        let merged_count = merged.len();
        let partitions = partition_and_rank(merged, &self.config)?;
        let limits = RetrievedContextLimits {
            max_role_audiences: ctx.budget().max_character_decisions(),
            max_items_per_audience: self.config.max_items_per_audience,
            max_tokens_per_audience: self.config.max_tokens_per_audience,
            max_total_items: self.config.max_total_items,
            max_total_tokens: self.config.max_total_tokens,
            max_item_bytes: self.config.max_item_bytes,
        };
        let trimmed = trim_round_robin(partitions, limits)?;
        let (world, mut characters) = split_partitions(trimmed);
        for (role_id, role_view) in role_views {
            characters.entry(role_id).or_default().role = Some(role_view);
        }
        let context = RetrievedContext::try_new(world, characters, limits).map_err(map_retrieved_context_error)?;
        let payload = serde_json::json!({
            "story_id": ctx.story_id(),
            "turn_number": ctx.turn_number().get(),
            "base_revision": snapshot.base_revision().get(),
            "character_request_count": plan.retrieval_plan.character_requests.len(),
            "knowledge_request_count": plan.retrieval_plan.knowledge_requests.len(),
            "fact_candidate_count": fact_candidate_count,
            "rumor_candidate_count": rumor_candidate_count,
            "memory_candidate_count": memory_candidate_count,
            "merged_count": merged_count,
            "world_item_count": context.world().facts.len() + context.world().rumors.len(),
            "character_partition_count": context.characters().len(),
            "total_tokens": context.total_tokens(),
            "status": "ok",
            "error_code": null,
        });
        ctx.trace().end_span_with(pending, &payload);
        ctx.set_retrieved_context(context)
    }
}

struct PartitionedItems {
    world: Vec<RetrievedKnowledgeItem>,
    characters: BTreeMap<RoleId, Vec<RetrievedKnowledgeItem>>,
}

fn merge_candidates(candidates: Vec<ContextCandidate>) -> Result<Vec<ContextCandidate>, TurnExecutionError> {
    let mut by_key: BTreeMap<(KnowledgeDelivery, KnowledgeSourceId), ContextCandidate> = BTreeMap::new();
    for candidate in candidates {
        let key = (candidate.delivery.clone(), candidate.record.source_id.clone());
        match by_key.get_mut(&key) {
            Some(existing) => {
                for (provider, evidence) in candidate.evidence {
                    match existing.evidence.get_mut(&provider) {
                        Some(existing_evidence) => {
                            existing_evidence.provider_rank =
                                existing_evidence.provider_rank.min(evidence.provider_rank);
                            for matched in evidence.matches {
                                if !existing_evidence.matches.contains(&matched) {
                                    existing_evidence.matches.push(matched);
                                }
                            }
                            existing_evidence.matches.sort();
                        }
                        None => {
                            existing.evidence.insert(provider, evidence);
                        }
                    }
                }
                existing.signal_priority = existing.signal_priority.min(candidate.signal_priority);
            }
            None => {
                by_key.insert(key, candidate);
            }
        }
    }
    let merged = by_key.into_values().collect::<Vec<_>>();
    if merged.iter().any(|candidate| {
        candidate.evidence.is_empty()
            || candidate
                .evidence
                .values()
                .any(|evidence| evidence.provider_rank == 0 || evidence.matches.is_empty())
    }) {
        return Err(map_context_error(ContextError::InvalidRecord {
            code: "candidate_evidence_invalid",
        }));
    }
    Ok(merged)
}

fn partition_and_rank(
    candidates: Vec<ContextCandidate>,
    config: &RetrievalConfig,
) -> Result<PartitionedItems, TurnExecutionError> {
    let mut world = Vec::new();
    let mut characters: BTreeMap<RoleId, Vec<RetrievedKnowledgeItem>> = BTreeMap::new();
    for candidate in candidates {
        let delivery = candidate.delivery.clone();
        if let KnowledgeDelivery::Character { role_id } = &delivery
            && candidate.record.kind == KnowledgeKind::Memory
            && candidate.record.memory_owner.as_ref() != Some(role_id)
        {
            return Err(map_retrieved_context_error(RetrievedContextError::InvalidMemoryOwner));
        }
        let item = candidate_to_item(candidate)?;
        match delivery {
            KnowledgeDelivery::Writer => world.push(item),
            KnowledgeDelivery::Character { role_id } => {
                characters.entry(role_id).or_default().push(item);
            }
        }
    }
    sort_partition(&mut world);
    for items in characters.values_mut() {
        sort_partition(items);
    }
    world.truncate(config.max_items_per_audience);
    trim_tokens(&mut world, config.max_tokens_per_audience);
    for items in characters.values_mut() {
        items.truncate(config.max_items_per_audience);
        trim_tokens(items, config.max_tokens_per_audience);
    }
    Ok(PartitionedItems { world, characters })
}

fn candidate_to_item(candidate: ContextCandidate) -> Result<RetrievedKnowledgeItem, TurnExecutionError> {
    let matches = candidate
        .evidence
        .values()
        .flat_map(|evidence| evidence.matches.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let match_level = match_level_from(&matches)?;
    let relevance = RelevanceRank {
        match_level,
        signal_priority: candidate.signal_priority,
        salience: candidate.record.salience,
    };
    Ok(RetrievedKnowledgeItem::from_parts(
        candidate.record.source_id,
        candidate.record.content,
        candidate.record.source,
        relevance,
        candidate.evidence,
    ))
}

fn match_level_from(matches: &[CandidateMatch]) -> Result<MatchLevel, TurnExecutionError> {
    let has_entity = matches.iter().any(|item| matches!(item, CandidateMatch::Entity(_)));
    let has_topic = matches.iter().any(|item| matches!(item, CandidateMatch::Topic(_)));
    match (has_entity, has_topic) {
        (true, true) => Ok(MatchLevel::EntityAndTopic),
        (true, false) => Ok(MatchLevel::Entity),
        (false, true) => Ok(MatchLevel::Topic),
        (false, false) => Err(map_context_error(ContextError::InvalidRecord {
            code: "candidate_match_missing",
        })),
    }
}

fn sort_partition(items: &mut [RetrievedKnowledgeItem]) {
    items.sort_by(|left, right| {
        rank_key(right)
            .cmp(&rank_key(left))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
}

fn rank_key(item: &RetrievedKnowledgeItem) -> (u8, u8, u8) {
    let level = match item.relevance.match_level {
        MatchLevel::EntityAndTopic => 2,
        MatchLevel::Entity => 1,
        MatchLevel::Topic => 0,
    };
    (level, u8::MAX - item.relevance.signal_priority, item.relevance.salience)
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

fn trim_round_robin(
    mut partitions: PartitionedItems,
    limits: RetrievedContextLimits,
) -> Result<PartitionedItems, TurnExecutionError> {
    let mut role_ids: Vec<RoleId> = partitions.characters.keys().cloned().collect();
    role_ids.sort();
    let mut world_idx = 0usize;
    let mut role_idxs: BTreeMap<RoleId, usize> = role_ids.iter().cloned().map(|id| (id, 0)).collect();
    let mut out_world = Vec::new();
    let mut out_characters: BTreeMap<RoleId, Vec<RetrievedKnowledgeItem>> = BTreeMap::new();
    let mut total_items = 0usize;
    let mut total_tokens = 0u64;
    loop {
        let mut progressed = false;
        if world_idx < partitions.world.len()
            && total_items < limits.max_total_items
            && out_world.len() < limits.max_items_per_audience
        {
            let item = &partitions.world[world_idx];
            let next_tokens = total_tokens.saturating_add(item.token_cost);
            let audience_tokens: u64 = out_world.iter().map(|item: &RetrievedKnowledgeItem| item.token_cost).sum();
            if next_tokens <= limits.max_total_tokens
                && audience_tokens.saturating_add(item.token_cost) <= limits.max_tokens_per_audience
            {
                out_world.push(partitions.world[world_idx].clone());
                world_idx += 1;
                total_items += 1;
                total_tokens = next_tokens;
                progressed = true;
            } else {
                world_idx = partitions.world.len();
            }
        }
        for role_id in &role_ids {
            let idx = role_idxs.get(role_id).copied().unwrap_or(0);
            let Some(source) = partitions.characters.get_mut(role_id) else {
                continue;
            };
            if idx >= source.len() || total_items >= limits.max_total_items {
                continue;
            }
            let out = out_characters.entry(role_id.clone()).or_default();
            if out.len() >= limits.max_items_per_audience {
                continue;
            }
            let item = &source[idx];
            let next_tokens = total_tokens.saturating_add(item.token_cost);
            let audience_tokens: u64 = out.iter().map(|item: &RetrievedKnowledgeItem| item.token_cost).sum();
            if next_tokens <= limits.max_total_tokens
                && audience_tokens.saturating_add(item.token_cost) <= limits.max_tokens_per_audience
            {
                out.push(source[idx].clone());
                role_idxs.insert(role_id.clone(), idx + 1);
                total_items += 1;
                total_tokens = next_tokens;
                progressed = true;
            } else {
                role_idxs.insert(role_id.clone(), source.len());
            }
        }
        if !progressed {
            break;
        }
    }
    Ok(PartitionedItems {
        world: out_world,
        characters: out_characters,
    })
}

fn split_partitions(
    partitions: PartitionedItems,
) -> (RetrievedWorldKnowledge, BTreeMap<RoleId, RetrievedCharacterContext>) {
    let mut world = RetrievedWorldKnowledge::default();
    for item in partitions.world {
        match item.source_id.kind() {
            KnowledgeKind::Fact => world.facts.push(item),
            KnowledgeKind::Rumor => world.rumors.push(item),
            KnowledgeKind::Memory => {}
        }
    }
    let mut characters = BTreeMap::new();
    for (role_id, items) in partitions.characters {
        let mut character = RetrievedCharacterContext::default();
        for item in items {
            match item.source_id.kind() {
                KnowledgeKind::Rumor => character.known_rumors.push(item),
                KnowledgeKind::Memory => character.memories.push(item),
                KnowledgeKind::Fact => {}
            }
        }
        characters.insert(role_id, character);
    }
    (world, characters)
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
