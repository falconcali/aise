use crate::domain::asset::ids::{AttributeKey, LocationKey, StoryRoleKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterInstanceState {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipState {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: crate::domain::asset::ids::RelationshipKind,
    pub trust: i16,
}
