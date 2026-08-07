use crate::domain::asset::ids::{CharacterAssetKey, SemanticVersion};
use crate::domain::asset::validation::BoundedText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub spec: CharacterSpec,
    pub spec_version: AssetSpecVersion,
    pub character_key: CharacterAssetKey,
    pub meta: CharacterMeta,
    pub profile: CharacterProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterSpec {
    #[serde(rename = "aise_char_v3")]
    V3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSpecVersion {
    #[serde(rename = "3.0")]
    V3_0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMeta {
    pub name: BoundedText,
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    pub description: BoundedText,
    pub personality: Vec<BoundedText>,
    pub values: Vec<BoundedText>,
    #[serde(default)]
    pub fears: Vec<BoundedText>,
    pub speaking_style: SpeakingStyle,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeakingStyle {
    pub register: BoundedText,
    pub verbosity: BoundedText,
    #[serde(default)]
    pub traits: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueExample {
    pub situation: BoundedText,
    pub response: BoundedText,
}
