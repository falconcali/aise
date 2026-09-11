use crate::config::ActivationIndexLimits;
use crate::domain::knowledge::activation::ActivationIndexSnapshot;
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ActivationIndexPort: Send + Sync {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexSnapshot>, StoreError>;
}
