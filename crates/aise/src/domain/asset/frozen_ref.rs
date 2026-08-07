use crate::domain::asset::ids::{
    AssetId, CharacterAssetKey, PackId, SemanticVersion, Sha256Digest, StoryPackKey, StoryRoleKey, WorldBookKey,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CharacterAssetSource {
    Embedded(Box<crate::domain::asset::character_card::CharacterCard>),
    Frozen(FrozenCharacterAssetRef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorldBookSource {
    Embedded(crate::domain::asset::world_book::WorldBook),
    Frozen(FrozenWorldBookRef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCharacterAssetRef {
    pub character_key: CharacterAssetKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenWorldBookRef {
    pub world_book_key: WorldBookKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultCast {
    pub character_ref: CharacterAssetKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticAssetDescriptor {
    pub path: String,
    pub mime_type: StaticMimeType,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticMimeType {
    Png,
    Jpeg,
    Webp,
    Gif,
    OggAudio,
    MpegAudio,
}

#[derive(Debug, Clone)]
pub struct FrozenStoryPackRef {
    pub pack_id: PackId,
    pub pack_key: StoryPackKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[allow(dead_code)]
pub(crate) fn _static_asset_anchor(_: &BTreeMap<AssetId, StaticAssetDescriptor>, _: &StoryRoleKey) {}
