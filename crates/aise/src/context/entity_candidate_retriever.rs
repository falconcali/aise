use crate::context::candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
use crate::context::error::ContextError;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::turn::CandidateRetrieverKind;
use crate::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeFilter, KnowledgeLookupHit, KnowledgeReadPort, SourceKnowledgeQuery,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct EntityCandidateRetriever {
    knowledge: Arc<dyn KnowledgeReadPort>,
}

impl EntityCandidateRetriever {
    pub fn new(knowledge: Arc<dyn KnowledgeReadPort>) -> Self {
        Self { knowledge }
    }
}

#[async_trait]
impl CandidateRetriever for EntityCandidateRetriever {
    fn kind(&self) -> CandidateRetrieverKind {
        CandidateRetrieverKind::Entity
    }

    async fn retrieve(&self, request: CandidateRetrievalRequest<'_>) -> Result<Vec<ContextCandidate>, ContextError> {
        if request.limit == 0 {
            return Ok(Vec::new());
        }
        authorize_request(request.request)?;
        let filter = KnowledgeFilter {
            delivery: request.request.delivery.clone(),
            knowledge_kinds: request.request.knowledge_kinds.clone(),
            max_item_bytes: request.max_item_bytes,
        };
        let records = if let Some(source_id) = &request.request.target_source_id {
            self.knowledge
                .find_by_source_ids(SourceKnowledgeQuery {
                    snapshot: request.snapshot,
                    filter: &filter,
                    source_ids: std::slice::from_ref(source_id),
                    limit: 1,
                })
                .await?
                .into_iter()
                .map(|record| KnowledgeLookupHit {
                    record,
                    matches: vec![crate::domain::knowledge::KnowledgeIndexMatch::Entity(
                        crate::domain::asset::entity::KnowledgeEntity::World(
                            crate::domain::asset::ids::EntityKey::from("exact_target"),
                        ),
                    )],
                })
                .collect()
        } else {
            if request.request.entities.is_empty() {
                return Ok(Vec::new());
            }
            self.knowledge
                .find_by_entities(EntityKnowledgeQuery {
                    snapshot: request.snapshot,
                    filter: &filter,
                    entities: &request.request.entities,
                    limit: request.limit,
                })
                .await?
        };
        let mut candidates = Vec::with_capacity(records.len());
        for (index, hit) in records.into_iter().enumerate() {
            let provider_rank = u32::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ContextError::CandidateLimitExceeded)?;
            candidates.push(ContextCandidate::from_hit(
                hit,
                request.request.delivery.clone(),
                CandidateRetrieverKind::Entity,
                provider_rank,
                request.request.signal_priority,
            )?);
        }
        Ok(candidates)
    }
}

fn authorize_request(request: &crate::domain::turn::KnowledgeRetrievalRequest) -> Result<(), ContextError> {
    match &request.delivery {
        crate::domain::turn::KnowledgeDelivery::Writer => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Memory) {
                return Err(ContextError::KnowledgeAudienceViolation);
            }
            Ok(())
        }
        crate::domain::turn::KnowledgeDelivery::Character { .. } => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Fact) {
                return Err(ContextError::KnowledgeAudienceViolation);
            }
            Ok(())
        }
    }
}
