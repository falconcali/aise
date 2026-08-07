use crate::domain::asset::character_card::CharacterCard;
use crate::domain::asset::frozen_ref::FrozenStoryPackRef;
use crate::domain::asset::ids::{CharacterAssetKey, PackId, SemanticVersion, Sha256Digest, StoryPackKey};
use crate::domain::asset::story_pack::StoryPack;
use crate::domain::asset::world_book::WorldBook;
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ValidatedStoryPack {
    pub pack: StoryPack,
    pub canonical_manifest: Vec<u8>,
    pub digest: Sha256Digest,
    pub resolved_characters: BTreeMap<CharacterAssetKey, CharacterCard>,
    pub resolved_world_book: WorldBook,
}

#[derive(Debug, Clone)]
pub struct FrozenStoryPack {
    pub pack_id: PackId,
    pub pack: StoryPack,
    pub digest: Sha256Digest,
    pub resolved_characters: BTreeMap<CharacterAssetKey, CharacterCard>,
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

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn find_pack_by_digest(&self, digest: &Sha256Digest) -> Result<Option<FrozenStoryPack>, StoreError>;
    async fn import_pack(&self, pack: ValidatedStoryPack) -> Result<FrozenStoryPack, StoreError>;
    async fn load_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError>;
    async fn export_pack(&self, pack_id: &PackId) -> Result<FrozenStoryPack, StoreError>;
    async fn list_packs(&self) -> Result<Vec<FrozenStoryPack>, StoreError>;
    async fn delete_pack(&self, pack_id: &PackId) -> Result<bool, StoreError>;
}
