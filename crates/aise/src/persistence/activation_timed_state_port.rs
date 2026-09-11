use crate::domain::knowledge::activation::ActivationTimedState;
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::store::StoreError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ActivationTimedStateQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub limit: usize,
}

#[async_trait]
pub trait ActivationTimedStateReadPort: Send + Sync {
    async fn load_timed_state(
        &self,
        query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError>;
}
