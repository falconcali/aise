use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use crate::core::{PackId, StoryPack};
use async_trait::async_trait;

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
    async fn get_pack(&self, pack_id: &PackId) -> Result<StoryPack, PersistenceError> {
        self.fake_store.get_pack(pack_id).await
    }
}
