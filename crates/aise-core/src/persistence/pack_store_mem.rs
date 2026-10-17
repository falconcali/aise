use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use crate::core::{PackRef, Sha256Digest, StoryId, StoryPack, StoryPackSnapshot};
use async_trait::async_trait;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub(crate) struct PackStoreMem {
    snapshots: RwLock<StorySnapshots>,
    config: PackStoreConfig,
}

#[derive(Debug, Default)]
struct StorySnapshots {
    packs: HashMap<Sha256Digest, Snapshot>,
    holders: HashMap<StoryId, Sha256Digest>,
}

#[derive(Debug)]
struct Snapshot {
    pack_ref: PackRef,
    value: Arc<StoryPack>,
    holders: BTreeSet<StoryId>,
}

impl PackStoreMem {
    pub(crate) fn new() -> Self {
        Self::with_config(PackStoreConfig::default())
    }

    pub(crate) fn with_config(config: PackStoreConfig) -> Self {
        Self {
            snapshots: RwLock::new(StorySnapshots::default()),
            config,
        }
    }
}

impl Default for PackStoreMem {
    fn default() -> Self {
        Self::new()
    }
}

impl StorySnapshots {
    fn retain(
        &mut self,
        story_id: &StoryId,
        snapshot: StoryPackSnapshot,
        config: &PackStoreConfig,
    ) -> Result<(), PersistenceError> {
        if self.holders.contains_key(story_id) {
            return Err(PersistenceError::StoryHolderAlreadyExists {
                story_id: story_id.clone(),
            });
        }
        let (pack_ref, pack) = snapshot.into_parts();
        if !self.packs.contains_key(&pack_ref.digest) && self.packs.len() >= config.max_packs {
            return Err(PersistenceError::PackCapacityExceeded {
                actual: self.packs.len().saturating_add(1),
                maximum: config.max_packs,
            });
        }
        let digest = pack_ref.digest.clone();
        if let Some(existing) = self.packs.get(&digest)
            && existing.pack_ref != pack_ref
        {
            return Err(PersistenceError::InvalidPackReference { pack_ref });
        }
        self.packs
            .entry(digest.clone())
            .or_insert_with(|| Snapshot {
                pack_ref,
                value: Arc::new(pack),
                holders: BTreeSet::new(),
            })
            .holders
            .insert(story_id.clone());
        self.holders.insert(story_id.clone(), digest);
        Ok(())
    }

    fn release(&mut self, story_id: &StoryId) {
        let Some(digest) = self.holders.remove(story_id) else {
            return;
        };
        if let Some(snapshot) = self.packs.get_mut(&digest) {
            snapshot.holders.remove(story_id);
            if snapshot.holders.is_empty() {
                self.packs.remove(&digest);
            }
        }
    }

    fn get(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        let snapshot = self.packs.get(&pack_ref.digest).ok_or(PersistenceError::NotFound)?;
        if snapshot.pack_ref != *pack_ref {
            return Err(PersistenceError::InvalidPackReference {
                pack_ref: pack_ref.clone(),
            });
        }
        Ok(Arc::clone(&snapshot.value))
    }
}

#[async_trait]
impl PackStore for PackStoreMem {
    async fn retain(&self, story_id: &StoryId, snapshot: StoryPackSnapshot) -> Result<(), PersistenceError> {
        self.snapshots.write().await.retain(story_id, snapshot, &self.config)
    }

    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        self.snapshots.write().await.release(story_id);
        Ok(())
    }

    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        self.snapshots.read().await.get(pack_ref)
    }
}

#[cfg(test)]
#[path = "tests/pack_store_mem_tests.rs"]
mod tests;
