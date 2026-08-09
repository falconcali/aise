use crate::context::candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
use crate::context::error::ContextError;
use crate::core::turn_data::{CandidateMatch, CandidateRetrieverKind};
use crate::domain::knowledge::KnowledgeKind;
use crate::persistence::knowledge_read_port::{EntityKnowledgeQuery, KnowledgeFilter, KnowledgeReadPort};
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
        if request.request.entities.is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        authorize_request(request.request)?;
        let filter = KnowledgeFilter {
            audience: request.request.audience.clone(),
            knowledge_kinds: request.request.knowledge_kinds.clone(),
            allowed_writer_memory_owners: request.allowed_writer_memory_owners.to_vec(),
        };
        let records = self
            .knowledge
            .find_by_entities(EntityKnowledgeQuery {
                snapshot: request.snapshot,
                filter: &filter,
                entities: &request.request.entities,
                limit: request.limit,
            })
            .await?;
        let mut candidates = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            let matches = request
                .request
                .entities
                .iter()
                .filter(|entity| record.entities.contains(entity))
                .cloned()
                .map(CandidateMatch::Entity)
                .collect::<Vec<_>>();
            candidates.push(ContextCandidate {
                record,
                audience: request.request.audience.clone(),
                retriever: CandidateRetrieverKind::Entity,
                provider_rank: (index as u32).saturating_add(1),
                matches,
                signal_priority: request.request.signal_priority,
            });
        }
        Ok(candidates)
    }
}

fn authorize_request(request: &crate::core::turn_data::RetrievalRequest) -> Result<(), ContextError> {
    match &request.audience {
        crate::core::turn_data::RetrievalAudience::GlobalWriter => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Memory) {
                let has_owner = request
                    .entities
                    .iter()
                    .any(|entity| matches!(entity, crate::domain::asset::entity::KnowledgeEntity::Character(_)));
                if !has_owner {
                    return Err(ContextError::KnowledgeAudienceViolation);
                }
            }
            Ok(())
        }
        crate::core::turn_data::RetrievalAudience::Character { .. } => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Fact) {
                return Err(ContextError::KnowledgeAudienceViolation);
            }
            Ok(())
        }
    }
}
