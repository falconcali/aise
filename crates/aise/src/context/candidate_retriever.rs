use crate::context::error::ContextError;
use crate::core::turn_data::{CandidateRetrieverKind, ProviderEvidence, RetrievalAudience, RetrievalRequest};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::knowledge_read_port::{KnowledgeLookupHit, KnowledgeRecord};
use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct CandidateRetrievalRequest<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub request: &'a RetrievalRequest,
    pub limit: usize,
    pub max_item_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ContextCandidate {
    pub record: KnowledgeRecord,
    pub audience: RetrievalAudience,
    pub evidence: BTreeMap<CandidateRetrieverKind, ProviderEvidence>,
    pub signal_priority: u8,
}

impl ContextCandidate {
    pub fn from_hit(
        hit: KnowledgeLookupHit,
        audience: RetrievalAudience,
        retriever: CandidateRetrieverKind,
        provider_rank: u32,
        signal_priority: u8,
    ) -> Result<Self, ContextError> {
        if provider_rank == 0 || hit.matches.is_empty() {
            return Err(ContextError::InvalidRecord {
                code: "candidate_evidence_invalid",
            });
        }
        Ok(Self {
            record: hit.record,
            audience,
            evidence: BTreeMap::from([(
                retriever,
                ProviderEvidence {
                    provider_rank,
                    matches: hit.matches,
                },
            )]),
            signal_priority,
        })
    }
}

#[async_trait]
pub trait CandidateRetriever: Send + Sync {
    fn kind(&self) -> CandidateRetrieverKind;

    async fn retrieve(&self, request: CandidateRetrievalRequest<'_>) -> Result<Vec<ContextCandidate>, ContextError>;
}
