use super::config::PackStoreConfig;
use super::error::PersistenceError;
use super::pack_store::PackStore;
use crate::core::{
    CharacterCard, CharacterCardId, CharacterCardRef, PackId, PackRef, SemanticVersion, Sha256Digest, StoryPack,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type CardKey = (CharacterCardId, SemanticVersion);

#[derive(Debug)]
pub struct PackStoreMem {
    assets: RwLock<StoredAssets>,
    config: PackStoreConfig,
}

#[derive(Debug, Default)]
struct StoredAssets {
    packs: HashMap<PackId, StoredPack>,
    pack_ids_by_digest: HashMap<Sha256Digest, PackId>,
    cards: HashMap<CardKey, StoredCard>,
}

#[derive(Debug)]
struct StoredPack {
    pack_ref: PackRef,
    pack: Arc<StoryPack>,
}

#[derive(Debug)]
struct StoredCard {
    card_ref: CharacterCardRef,
    card: Arc<CharacterCard>,
}

impl PackStoreMem {
    pub fn new() -> Self {
        Self::with_config(PackStoreConfig::default())
    }

    pub fn with_config(config: PackStoreConfig) -> Self {
        Self {
            assets: RwLock::new(StoredAssets::default()),
            config,
        }
    }
}

impl Default for PackStoreMem {
    fn default() -> Self {
        Self::new()
    }
}

impl StoredAssets {
    fn insert_pack(
        &mut self,
        pack_ref: PackRef,
        pack: StoryPack,
        max_packs: usize,
    ) -> Result<PackRef, PersistenceError> {
        if let Some(existing) = self
            .pack_ids_by_digest
            .get(&pack_ref.digest)
            .and_then(|pack_id| self.packs.get(pack_id))
        {
            return Ok(existing.pack_ref.clone());
        }
        if self.packs.contains_key(&pack_ref.pack_id) {
            return Err(constraint_violation("pack_id already exists"));
        }
        if self.packs.len() >= max_packs {
            return Err(constraint_violation("pack store capacity reached"));
        }
        self.pack_ids_by_digest
            .insert(pack_ref.digest.clone(), pack_ref.pack_id.clone());
        self.packs.insert(
            pack_ref.pack_id.clone(),
            StoredPack {
                pack_ref: pack_ref.clone(),
                pack: Arc::new(pack),
            },
        );
        Ok(pack_ref)
    }

    fn insert_card(
        &mut self,
        card_ref: CharacterCardRef,
        card: CharacterCard,
        max_cards: usize,
    ) -> Result<CharacterCardRef, PersistenceError> {
        let key = (card_ref.card_id.clone(), card_ref.version.clone());
        if let Some(existing) = self.cards.get(&key) {
            if existing.card_ref.digest == card_ref.digest {
                return Ok(existing.card_ref.clone());
            }
            return Err(constraint_violation(
                "character card version already exists with different content",
            ));
        }
        if self.cards.len() >= max_cards {
            return Err(constraint_violation("character card store capacity reached"));
        }
        self.cards.insert(
            key,
            StoredCard {
                card_ref: card_ref.clone(),
                card: Arc::new(card),
            },
        );
        Ok(card_ref)
    }

    fn find_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError> {
        let key = (card_ref.card_id.clone(), card_ref.version.clone());
        match self.cards.get(&key) {
            Some(stored) if stored.card_ref.digest == card_ref.digest => Ok(Arc::clone(&stored.card)),
            Some(_) => Err(constraint_violation("character card digest mismatch")),
            None => Err(PersistenceError::NotFound),
        }
    }
}

fn constraint_violation(constraint: &str) -> PersistenceError {
    PersistenceError::ConstraintViolation {
        constraint: constraint.to_owned(),
    }
}

#[async_trait]
impl PackStore for PackStoreMem {
    async fn import_pack(&self, pack_id: PackId, pack: StoryPack) -> Result<PackRef, PersistenceError> {
        let pack_ref = PackRef {
            pack_id,
            version: pack.meta.version.clone(),
            digest: pack.digest()?,
        };
        self.assets.write().await.insert_pack(pack_ref, pack, self.config.max_packs)
    }

    async fn get_pack(&self, pack_id: &PackId) -> Result<Arc<StoryPack>, PersistenceError> {
        let assets = self.assets.read().await;
        assets
            .packs
            .get(pack_id)
            .map(|stored| Arc::clone(&stored.pack))
            .ok_or(PersistenceError::NotFound)
    }

    async fn import_character_card(&self, card: CharacterCard) -> Result<CharacterCardRef, PersistenceError> {
        let card_ref = CharacterCardRef {
            card_id: card.card_id.clone(),
            version: card.meta.version.clone(),
            digest: card.digest()?,
        };
        self.assets
            .write()
            .await
            .insert_card(card_ref, card, self.config.max_character_cards)
    }

    async fn get_character_card(&self, card_ref: &CharacterCardRef) -> Result<Arc<CharacterCard>, PersistenceError> {
        self.assets.read().await.find_card(card_ref)
    }
}

#[cfg(test)]
#[path = "tests/pack_store_mem_tests.rs"]
mod tests;
