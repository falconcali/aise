use crate::core::story::{StoryCommit, StoryContext, StoryInstanceInfo, StoryInstanceSpec};
use crate::core::{IdempotencyKey, StoryId};
use super::error::PersistenceError;
use async_trait::async_trait;

#[async_trait]
pub trait StoryStore: Send + Sync {
    async fn create(&self, spec: &StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn get_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn load(&self, story_id: &StoryId) -> Result<StoryContext, PersistenceError>;
    async fn find_committed(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError>;
    async fn commit(&self, commit: &StoryCommit) -> Result<StoryCommit, PersistenceError>;
}