use crate::domain::asset::ids::SemanticVersion;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterSpec {
    #[serde(rename = "aise_char_v4")]
    V4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSpecVersion {
    #[serde(rename = "3.0")]
    V3_0,
    #[serde(rename = "4.0")]
    V4_0,
    #[serde(rename = "5.0")]
    V5_0,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub spec: CharacterSpec,
    pub spec_version: AssetSpecVersion,
    pub character_id: CharacterId,
    pub meta: CharacterMeta,
    pub profile: CharacterProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMeta {
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    pub name: BoundedText,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueExample {
    pub situation: BoundedText,
    pub response: BoundedText,
}
