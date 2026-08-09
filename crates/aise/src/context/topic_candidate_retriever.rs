use crate::context::candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
use crate::context::error::ContextError;
use crate::core::turn_data::{CandidateMatch, CandidateRetrieverKind, RetrievalAudience, RetrievalRequest};
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::knowledge::KnowledgeKind;
use crate::persistence::knowledge_read_port::{KnowledgeFilter, KnowledgeReadPort, TopicKnowledgeQuery};
use async_trait::async_trait;
use std::sync::Arc;

pub struct TopicCandidateRetriever {
    knowledge: Arc<dyn KnowledgeReadPort>,
}

impl TopicCandidateRetriever {
    pub fn new(knowledge: Arc<dyn KnowledgeReadPort>) -> Self {
        Self { knowledge }
    }
}

#[async_trait]
impl CandidateRetriever for TopicCandidateRetriever {
    fn kind(&self) -> CandidateRetrieverKind {
        CandidateRetrieverKind::Topic
    }

    async fn retrieve(&self, request: CandidateRetrievalRequest<'_>) -> Result<Vec<ContextCandidate>, ContextError> {
        if request.request.topics.is_empty() || request.limit == 0 {
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
            .find_by_topics(TopicKnowledgeQuery {
                snapshot: request.snapshot,
                filter: &filter,
                topics: &request.request.topics,
                limit: request.limit,
            })
            .await?;
        let mut candidates = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            let matches = request
                .request
                .topics
                .iter()
                .filter(|topic| record.topics.contains(topic))
                .cloned()
                .map(CandidateMatch::Topic)
                .collect::<Vec<_>>();
            candidates.push(ContextCandidate {
                record,
                audience: request.request.audience.clone(),
                retriever: CandidateRetrieverKind::Topic,
                provider_rank: (index as u32).saturating_add(1),
                matches,
                signal_priority: request.request.signal_priority,
            });
        }
        Ok(candidates)
    }
}

fn authorize_request(request: &RetrievalRequest) -> Result<(), ContextError> {
    match &request.audience {
        RetrievalAudience::GlobalWriter => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Memory) {
                let has_owner = request
                    .entities
                    .iter()
                    .any(|entity| matches!(entity, KnowledgeEntity::Character(_)));
                if !has_owner {
                    return Err(ContextError::KnowledgeAudienceViolation);
                }
            }
            Ok(())
        }
        RetrievalAudience::Character { .. } => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Fact) {
                return Err(ContextError::KnowledgeAudienceViolation);
            }
            Ok(())
        }
    }
}
