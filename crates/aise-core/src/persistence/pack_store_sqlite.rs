use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use crate::core::{PackRef, StoryId, StoryPack, StoryPackSnapshot};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct PackStoreSqlite {
    fake_store: PackStoreMem,
}

impl PackStoreSqlite {
    pub(crate) fn new(config: PackStoreConfig) -> Self {
        Self {
            fake_store: PackStoreMem::with_config(config),
        }
    }
}

impl Default for PackStoreSqlite {
    fn default() -> Self {
        Self::new(PackStoreConfig::default())
    }
}

#[async_trait]
impl PackStore for PackStoreSqlite {
    async fn retain(&self, story_id: &StoryId, snapshot: StoryPackSnapshot) -> Result<(), PersistenceError> {
        self.fake_store.retain(story_id, snapshot).await
    }

    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        self.fake_store.release(story_id).await
    }

    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        self.fake_store.get_pack(pack_ref).await
    }
}
