use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use crate::core::{CharacterCard, CharacterCardRef, PackRef, Sha256Digest, StoryAssetSpec, StoryId, StoryPack};
use async_trait::async_trait;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct PackStoreMem {
    snapshots: RwLock<StorySnapshots>,
    config: PackStoreConfig,
}

#[derive(Debug, Default)]
struct StorySnapshots {
    packs: SnapshotTable<StoryPack>,
    cards: SnapshotTable<CharacterCard>,
}

#[derive(Debug)]
struct SnapshotTable<T> {
    entries: HashMap<Sha256Digest, Snapshot<T>>,
}

#[derive(Debug)]
struct Snapshot<T> {
    value: Arc<T>,
    holders: BTreeSet<StoryId>,
}

impl PackStoreMem {
    pub fn new() -> Self {
        Self::with_config(PackStoreConfig::default())
    }

    pub fn with_config(config: PackStoreConfig) -> Self {
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
        assets: StoryAssetSpec,
        config: &PackStoreConfig,
    ) -> Result<(), PersistenceError> {
        self.packs.check_capacity(
            std::iter::once(&assets.pack_ref.digest),
            config.max_packs,
            "pack store capacity reached",
        )?;
        self.cards.check_capacity(
            assets.cards.iter().map(|(card_ref, _)| &card_ref.digest),
            config.max_character_cards,
            "character card store capacity reached",
        )?;
        self.packs.retain(story_id, assets.pack_ref.digest, assets.pack);
        for (card_ref, card) in assets.cards {
            self.cards.retain(story_id, card_ref.digest, card);
        }
        Ok(())
    }

    fn release(&mut self, story_id: &StoryId) {
        self.packs.release(story_id);
        self.cards.release(story_id);
    }
}

impl<T> Default for SnapshotTable<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> SnapshotTable<T> {
    fn check_capacity<'a>(
        &self,
        digests: impl Iterator<Item = &'a Sha256Digest>,
        maximum: usize,
        constraint: &str,
    ) -> Result<(), PersistenceError> {
        let incoming = digests
            .filter(|digest| !self.entries.contains_key(*digest))
            .collect::<BTreeSet<_>>()
            .len();
        if self.entries.len() + incoming > maximum {
            return Err(PersistenceError::ConstraintViolation {
                constraint: constraint.to_owned(),
            });
        }
        Ok(())
    }

    fn retain(&mut self, story_id: &StoryId, digest: Sha256Digest, value: T) {
        self.entries
            .entry(digest)
            .or_insert_with(|| Snapshot {
                value: Arc::new(value),
                holders: BTreeSet::new(),
            })
            .holders
            .insert(story_id.clone());
    }

    fn release(&mut self, story_id: &StoryId) {
        self.entries.retain(|_, snapshot| {
            snapshot.holders.remove(story_id);
            !snapshot.holders.is_empty()
        });
    }

    fn get(&self, digest: &Sha256Digest) -> Result<Arc<T>, PersistenceError> {
        self.entries
            .get(digest)
            .map(|snapshot| Arc::clone(&snapshot.value))
            .ok_or(PersistenceError::NotFound)
    }
}

#[async_trait]
impl PackStore for PackStoreMem {
    async fn retain(&self, story_id: &StoryId, assets: StoryAssetSpec) -> Result<(), PersistenceError> {
        self.snapshots.write().await.retain(story_id, assets, &self.config)
    }

    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError> {
        self.snapshots.write().await.release(story_id);
        Ok(())
    }

    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        self.snapshots.read().await.packs.get(&pack_ref.digest)
    }

    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError> {
        self.snapshots.read().await.cards.get(&card_ref.digest)
    }
}

#[cfg(test)]
#[path = "tests/pack_store_mem_tests.rs"]
mod tests;
