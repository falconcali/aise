use super::config::StoryStoreConfig;
use super::error::PersistenceError;
use super::story_store::StoryStore;
use super::story_store_mem::StoryStoreMem;
use crate::core::{
    IdempotencyKey, PackId, StoryCommit, StoryContext, StoryId, StoryInstanceInfo, StoryInstanceSpec, StoryPack,
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct StoryStoreSqlite {
    fake_store: StoryStoreMem,
}

impl StoryStoreSqlite {
    pub fn new(config: StoryStoreConfig) -> Self {
        Self {
            fake_store: StoryStoreMem::with_config(config),
        }
    }
}

impl Default for StoryStoreSqlite {
    fn default() -> Self {
        Self::new(StoryStoreConfig::default())
    }
}

#[async_trait]
impl StoryStore for StoryStoreSqlite {
    async fn create(&self, spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError> {
        self.fake_store.create(spec).await
    }

    async fn remove(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        self.fake_store.remove(story_id).await
    }

    async fn get_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError> {
        self.fake_store.get_info(story_id).await
    }

    async fn load(&self, story_id: &StoryId) -> Result<StoryContext, PersistenceError> {
        self.fake_store.load(story_id).await
    }

    async fn find_committed(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError> {
        self.fake_store.find_committed(story_id, idempotency_key).await
    }

    async fn commit(&self, commit: &StoryCommit) -> Result<StoryCommit, PersistenceError> {
        self.fake_store.commit(commit).await
    }

    async fn get_pack(&self, pack_id: &PackId) -> Result<StoryPack, PersistenceError> {
        self.fake_store.get_pack(pack_id).await
    }
}
