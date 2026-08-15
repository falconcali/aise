use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::frozen_ref::{FrozenCharacterCardRef, FrozenStoryPackRef};
use crate::domain::asset::ids::{PackId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::StoryPack;
use crate::domain::asset::validation::BoundedText;
use crate::domain::asset::world_book::WorldBook;
use crate::domain::ids::CharacterId;
use crate::persistence::store::StoreError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ValidatedStoryPack {
    pub pack: StoryPack,
    pub canonical_manifest: Vec<u8>,
    pub digest: Sha256Digest,
    pub resolved_world_book: WorldBook,
}

#[derive(Debug, Clone)]
pub struct FrozenStoryPack {
    pub pack_id: PackId,
    pub pack: StoryPack,
    pub digest: Sha256Digest,
    pub resolved_world_book: WorldBook,
}

impl FrozenStoryPack {
    pub fn frozen_ref(&self) -> FrozenStoryPackRef {
        FrozenStoryPackRef {
            pack_id: self.pack_id.clone(),
            pack_key: self.pack.meta.pack_key.clone(),
            version: self.pack.meta.version.clone(),
            digest: self.digest.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackInfo {
    pub pack_id: PackId,
    pub pack_key: StoryPackKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct ValidatedCharacterCard {
    pub card: CharacterCard,
    pub canonical_json: Vec<u8>,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct FrozenCharacterCard {
    pub card: CharacterCard,
    pub digest: Sha256Digest,
}

impl FrozenCharacterCard {
    pub fn frozen_ref(&self) -> FrozenCharacterCardRef {
        FrozenCharacterCardRef {
            character_id: self.card.character_id.clone(),
            version: self.card.meta.version.clone(),
            digest: self.digest.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharacterCardInfo {
    pub character_id: CharacterId,
    pub name: BoundedText,
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn find_pack_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenStoryPack>, StoreError>;
    async fn import_pack(&self, pack: ValidatedStoryPack) -> Result<FrozenStoryPack, StoreError>;
    async fn load_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError>;
    async fn export_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError>;
    async fn list_packs(&self) -> Result<Vec<FrozenStoryPack>, StoreError>;
    async fn delete_pack(&self, pack_id: &PackId) -> Result<bool, StoreError>;

    async fn find_character_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenCharacterCard>, StoreError>;
    async fn import_character(&self, value: ValidatedCharacterCard) -> Result<FrozenCharacterCard, StoreError>;
    async fn load_character(&self, reference: &FrozenCharacterCardRef) -> Result<FrozenCharacterCard, StoreError>;
    async fn list_characters(&self) -> Result<Vec<CharacterCardInfo>, StoreError>;
}
