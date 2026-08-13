use crate::domain::asset::character_card::AssetSpecVersion;
use crate::domain::asset::constraint::StoryConstraintDefinition;
use crate::domain::asset::frozen_ref::{DefaultCast, StaticAssetDescriptor, WorldBookSource};
use crate::domain::asset::ids::{
    AssetId, AttributeKey, CharacterAssetKey, ConstraintKey, LocationKey, MemoryKey, RelationshipKind, SceneKey,
    SemanticVersion, StoryPackKey, StoryRoleKey, TopicKey,
};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::narrative_graph::definition::NarrativeGraphDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StorySpec,
    pub spec_version: AssetSpecVersion,
    pub meta: StoryPackMeta,
    pub story: StoryProfile,
    pub character_assets: BTreeMap<CharacterAssetKey, crate::domain::asset::frozen_ref::CharacterAssetSource>,
    pub roles: BTreeMap<StoryRoleKey, StoryRole>,
    pub default_cast: BTreeMap<StoryRoleKey, DefaultCast>,
    pub play: PlayDefinition,
    pub world_book: WorldBookSource,
    pub start: StoryStart,
    pub narrative: NarrativeGraphDefinition,
    #[serde(default)]
    pub constraints: BTreeMap<ConstraintKey, StoryConstraintDefinition>,
    #[serde(default)]
    pub assets: BTreeMap<AssetId, StaticAssetDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorySpec {
    #[serde(rename = "aise_story_v3")]
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPackMeta {
    pub pack_key: StoryPackKey,
    pub title: BoundedText,
    pub author: BoundedText,
    pub version: SemanticVersion,
    pub description: BoundedText,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
    pub cover_asset: Option<AssetId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProfile {
    pub premise: BoundedText,
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub style: StoryStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStyle {
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRole {
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub initial_state: InitialRoleState,
    #[serde(default)]
    pub initial_relationships: Vec<RelationshipSeed>,
    #[serde(default)]
    pub seed_memories: Vec<MemorySeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialRoleState {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipSeed {
    pub target_role_key: StoryRoleKey,
    pub kind: RelationshipKind,
    pub trust: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySeed {
    pub memory_key: MemoryKey,
    pub kind: crate::domain::asset::ids::MemoryKind,
    pub content: BoundedText,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub player_count: u16,
    pub playable_role_keys: Vec<StoryRoleKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStart {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub time: BoundedText,
    pub description: BoundedText,
    pub opening: BoundedText,
}
