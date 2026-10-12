use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use crate::core::{CharacterCard, CharacterCardRef, PackRef, StoryAssetSpec, StoryId, StoryPack};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug)]
pub struct PackStoreSqlite {
    fake_store: PackStoreMem,
}

impl PackStoreSqlite {
    pub fn new(config: PackStoreConfig) -> Self {
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
    async fn retain(&self, story_id: &StoryId, assets: StoryAssetSpec) -> Result<(), PersistenceError> {
        self.fake_store.retain(story_id, assets).await
    }

    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        self.fake_store.release(story_id).await
    }

    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        self.fake_store.get_pack(pack_ref).await
    }

    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError> {
        self.fake_store.get_character_card(card_ref).await
    }
}
