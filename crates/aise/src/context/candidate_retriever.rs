use crate::context::error::ContextError;
use crate::core::turn_data::{CandidateMatch, CandidateRetrieverKind, RetrievalAudience, RetrievalRequest};
use crate::domain::ids::CharacterId;
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::knowledge_read_port::KnowledgeRecord;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CandidateRetrievalRequest<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub request: &'a RetrievalRequest,
    pub allowed_writer_memory_owners: &'a [CharacterId],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ContextCandidate {
    pub record: KnowledgeRecord,
    pub audience: RetrievalAudience,
    pub retriever: CandidateRetrieverKind,
    pub provider_rank: u32,
    pub matches: Vec<CandidateMatch>,
    pub signal_priority: u8,
}

#[async_trait]
pub trait CandidateRetriever: Send + Sync {
    fn kind(&self) -> CandidateRetrieverKind;

    async fn retrieve(&self, request: CandidateRetrievalRequest<'_>) -> Result<Vec<ContextCandidate>, ContextError>;
}
