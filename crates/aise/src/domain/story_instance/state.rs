use crate::domain::asset::ids::{
    AttributeKey, InstanceSettingKey, LocationKey, RelationshipKind, SceneKey, StoryRoleKey,
};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceSettings {
    #[serde(default)]
    pub values: BTreeMap<InstanceSettingKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentScene {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub time: BoundedText,
    pub description: BoundedText,
    pub present_character_ids: Vec<CharacterId>,
}

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
    pub kind: RelationshipKind,
    pub trust: i16,
}
