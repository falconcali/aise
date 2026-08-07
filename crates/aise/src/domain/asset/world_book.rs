use crate::domain::asset::character_card::AssetSpecVersion;
use crate::domain::asset::ids::{EntityKey, FactKey, RumorKey, SemanticVersion, StoryRoleKey, TopicKey, WorldBookKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBook {
    pub spec: WorldSpec,
    pub spec_version: AssetSpecVersion,
    pub world_book_key: WorldBookKey,
    pub meta: WorldBookMeta,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldSpec {
    #[serde(rename = "aise_world_v3")]
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBookMeta {
    pub name: BoundedText,
    pub version: SemanticVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<EntityKey>,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<EntityKey>,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: EntityRef,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case")]
pub enum EntityRef {
    World(EntityKey),
    Role(StoryRoleKey),
}
