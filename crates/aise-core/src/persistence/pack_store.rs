use super::error::PersistenceError;
use crate::core::{PackRef, StoryId, StoryPack, StoryPackSnapshot};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub(crate) trait PackStore: Send + Sync {
    async fn retain(&self, story_id: &StoryId, snapshot: StoryPackSnapshot) -> Result<(), PersistenceError>;
    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError>;
    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError>;
}
