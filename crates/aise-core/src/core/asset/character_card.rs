use crate::core::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub character_id: CharacerId,
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