use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use crate::core::{CharacterCard, CharacterCardRef, PackId, PackRef, StoryPack};
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
    async fn import_pack(&self, pack_id: PackId, pack: StoryPack) -> Result<PackRef, PersistenceError> {
        self.fake_store.import_pack(pack_id, pack).await
    }

    async fn get_pack(&self, pack_id: &PackId) -> Result<Arc<StoryPack>, PersistenceError> {
        self.fake_store.get_pack(pack_id).await
    }

    async fn import_character_card(&self, card: CharacterCard) -> Result<CharacterCardRef, PersistenceError> {
        self.fake_store.import_character_card(card).await
    }

    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError> {
        self.fake_store.get_character_card(card_ref).await
    }
}
