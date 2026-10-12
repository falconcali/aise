use super::error::PersistenceError;
use crate::core::{PackId, StoryPack};
use async_trait::async_trait;

#[async_trait]
pub trait PackStore: Send + Sync {
    async fn get_pack(&self, pack_id: &PackId) -> Result<StoryPack, PersistenceError>;
}
