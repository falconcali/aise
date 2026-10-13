use crate::core::{BoundedText, CharacterCardId, CharacterProfile, CoreError, SemanticVersion, Sha256Digest};
use serde::{Deserialize, Serialize};

const CHARACTER_CARD_SUBJECT: &str = "character_card";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardRef {
    pub card_id: CharacterCardId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

impl CharacterCard {
    pub fn card_ref(&self) -> Result<CharacterCardRef, CoreError> {
        Ok(CharacterCardRef {
            card_id: self.card_id.clone(),
            version: self.meta.version.clone(),
            digest: Sha256Digest::canonical(CHARACTER_CARD_SUBJECT, self)?,
        })
    }
}

#[cfg(test)]
#[path = "tests/character_card_tests.rs"]
mod tests;
