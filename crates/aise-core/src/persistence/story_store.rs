use super::error::PersistenceError;
use crate::core::{
    IdempotencyKey, PackId, StoryCommit, StoryContext, StoryId, StoryInstanceInfo, StoryInstanceSpec, StoryPack,
};
use async_trait::async_trait;

#[async_trait]
pub trait StoryStore: Send + Sync {
    async fn create(&self, spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn remove(&self, story_id: &StoryId) -> Result<(), PersistenceError>;
    async fn get_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn load(&self, story_id: &StoryId) -> Result<StoryContext, PersistenceError>;
    async fn find_committed(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError>;
    async fn commit(&self, commit: &StoryCommit) -> Result<StoryCommit, PersistenceError>;

    async fn get_pack(&self, pack_id: &PackId) -> Result<StoryPack, PersistenceError>;
}
