use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::knowledge::activation::{
    ActivationIndexSnapshot, ExternalActivationSeed, GenerationTrigger, ScanFragment,
};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use async_trait::async_trait;

pub struct ActivationSeedRequest<'a> {
    pub story_id: &'a StoryId,
    pub turn_number: TurnNumber,
    pub generation_trigger: GenerationTrigger,
    pub knowledge_snapshot: &'a KnowledgeSnapshotRef,
    pub index_snapshot: &'a ActivationIndexSnapshot,
    pub scan_fragments: &'a [ScanFragment],
    pub max_seeds: usize,
}

#[async_trait]
pub trait ActivationSeedProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn seeds(&self, request: ActivationSeedRequest<'_>) -> Result<Vec<ExternalActivationSeed>, ProviderError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("activation seed provider is unavailable")]
    Unavailable,
    #[error("activation seed provider exceeded its seed budget")]
    SeedLimitExceeded,
    #[error("activation seed provider returned an unauthorized target")]
    UnauthorizedTarget,
}
