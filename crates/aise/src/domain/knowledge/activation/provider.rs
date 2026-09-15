use super::contracts::ActivationEvidence;
use super::scan::ActivationScanBuffer;
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::KnowledgeDelivery;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ActivationSeedRequest<'a> {
    pub knowledge_snapshot: &'a KnowledgeSnapshotRef,
    pub scan_buffer: &'a ActivationScanBuffer,
    pub query_text: Option<&'a BoundedText>,
    pub allowed_kinds: &'a [KnowledgeKind],
    pub delivery: &'a KnowledgeDelivery,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ProviderActivationCandidate {
    pub source_id: KnowledgeSourceId,
    pub provider_rank: u32,
    pub evidence: ActivationEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationProviderError {
    #[error("activation provider request is invalid: {code}")]
    InvalidRequest { code: &'static str },
    #[error("activation provider limit exceeded")]
    LimitExceeded,
    #[error("activation provider is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait ActivationSeedProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;

    async fn candidates(
        &self,
        request: ActivationSeedRequest<'_>,
    ) -> Result<Vec<ProviderActivationCandidate>, ActivationProviderError>;
}

#[cfg(test)]
#[path = "tests/provider_tests.rs"]
mod tests;
