use crate::core::{BoundedText, CharacterCardId, CharacterProfile, SemanticVersion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub spec: CharacterCardSpec,
    pub card_id: CharacterCardId,
    pub name: BoundedText,
    pub meta: CharacterMeta,
    pub profile: CharacterProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterCardSpec {
    #[serde(rename = "aise_char_v1")]
    V1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMeta {
    pub creator: BoundedText,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}

#[cfg(test)]
#[path = "tests/character_card_tests.rs"]
mod tests;
