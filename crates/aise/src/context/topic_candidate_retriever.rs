use crate::context::candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
use crate::context::error::ContextError;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::turn::{CandidateRetrieverKind, RetrievalAudience, RetrievalRequest};
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
            authorized_memory_owners: request.request.authorized_memory_owners.clone(),
            max_item_bytes: request.max_item_bytes,
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
        for (index, hit) in records.into_iter().enumerate() {
            let provider_rank = u32::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ContextError::CandidateLimitExceeded)?;
            candidates.push(ContextCandidate::from_hit(
                hit,
                request.request.audience.clone(),
                CandidateRetrieverKind::Topic,
                provider_rank,
                request.request.signal_priority,
            )?);
        }
        Ok(candidates)
    }
}

fn authorize_request(request: &RetrievalRequest) -> Result<(), ContextError> {
    match &request.audience {
        RetrievalAudience::GlobalWriter => {
            if request.knowledge_kinds.contains(&KnowledgeKind::Memory) && request.authorized_memory_owners.is_empty() {
                return Err(ContextError::KnowledgeAudienceViolation);
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
