use super::config::{PackStoreConfig, PersistanceConfig, StoreType, StoryStoreConfig};
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use super::pack_store_sqlite::PackStoreSqlite;
use super::story_store::StoryStore;
use super::story_store_mem::StoryStoreMem;
use super::story_store_sqlite::StoryStoreSqlite;

pub struct Store {
    story_store: Box<dyn StoryStore>,
    pack_store: Box<dyn PackStore>,
}

impl Store {
    pub fn new(config: PersistanceConfig) -> Self {
        Self {
            story_store: Self::create_story_store(config.story_store),
            pack_store: Self::create_pack_store(config.pack_store),
        }
    }

    pub fn story_store(&self) -> &dyn StoryStore {
        self.story_store.as_ref()
    }

    pub fn pack_store(&self) -> &dyn PackStore {
        self.pack_store.as_ref()
    }

    fn create_story_store(config: StoryStoreConfig) -> Box<dyn StoryStore> {
        match config.store_type {
            StoreType::Memory => Box::new(StoryStoreMem::with_config(config)),
            StoreType::Sqlite => Box::new(StoryStoreSqlite::new(config)),
        }
    }

    fn create_pack_store(config: PackStoreConfig) -> Box<dyn PackStore> {
        match config.store_type {
            StoreType::Memory => Box::new(PackStoreMem::with_config(config)),
            StoreType::Sqlite => Box::new(PackStoreSqlite::new(config)),
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new(PersistanceConfig::default())
    }
}

#[cfg(test)]
#[path = "tests/store_tests.rs"]
mod tests;
