use super::config::{PackStoreConfig, PersistanceConfig, StoreType, StoryStoreConfig};
use super::pack_store::PackStore;
use super::pack_store_mem::PackStoreMem;
use super::pack_store_sqlite::PackStoreSqlite;
use super::story_store::StoryStore;
use super::story_store_mem::StoryStoreMem;
use super::story_store_sqlite::StoryStoreSqlite;
use crate::core::{StoryCreateBundle, StoryId, StoryInstanceInfo};

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

    pub(crate) async fn create_story(
        &self,
        bundle: StoryCreateBundle,
    ) -> Result<StoryInstanceInfo, super::PersistenceError> {
        let (snapshot, instance) = bundle.into_parts();
        let story_id = instance.story_id().clone();
        self.pack_store.retain(&story_id, snapshot).await?;
        match self.story_store.create(instance).await {
            Ok(info) => Ok(info),
            Err(create_source) => match self.pack_store.release(&story_id).await {
                Ok(()) => Err(create_source),
                Err(cleanup_source) => {
                    let error = super::PersistenceError::CreateCompensationFailed {
                        story_id: story_id.clone(),
                        create_source: Box::new(create_source),
                        cleanup_source: Box::new(cleanup_source),
                    };
                    tracing::error!(
                        target: "aise::persistence",
                        story_id = %story_id,
                        error = %error,
                        "story creation compensation failed"
                    );
                    Err(error)
                }
            },
        }
    }

    pub(crate) async fn remove_story(&self, story_id: &StoryId) -> Result<(), super::PersistenceError> {
        match self.story_store.remove(story_id).await {
            Ok(()) | Err(super::PersistenceError::NotFound) => {}
            Err(error) => return Err(error),
        }
        if let Err(error) = self.pack_store.release(story_id).await {
            tracing::error!(
                target: "aise::persistence",
                story_id = %story_id,
                error = %error,
                "story pack release failed"
            );
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn story_store(&self) -> &dyn StoryStore {
        self.story_store.as_ref()
    }

    pub(crate) fn pack_store(&self) -> &dyn PackStore {
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
