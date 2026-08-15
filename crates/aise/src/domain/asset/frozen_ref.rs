use crate::domain::asset::ids::{PackId, SemanticVersion, Sha256Digest, StoryPackKey, WorldBookKey};
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorldBookSource {
    Embedded(crate::domain::asset::world_book::WorldBook),
    Frozen(FrozenWorldBookRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCharacterCardRef {
    pub character_id: CharacterId,
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
