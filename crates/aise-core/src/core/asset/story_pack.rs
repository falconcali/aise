use super::CharacterProfile;
use crate::core::{BoundedText, FactBody, MemoryBody, RoleId, RumorBody, SemanticVersion};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StoryPackSpec,
    pub meta: StoryPackMeta,
    pub profile: StoryPackProfile,
    #[serde(default)]
    pub roles: BTreeMap<RoleId, RoleProfile>,
    pub play: PlayDefinition,
    pub opening: BoundedText,
    #[serde(default)]
    pub seed_facts: Vec<FactBody>,
    #[serde(default)]
    pub seed_rumors: Vec<RumorBody>,
    #[serde(default)]
    pub constraints: Vec<BoundedText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryPackSpec {
    #[serde(rename = "aise_story_v1")]
    V1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPackMeta {
    pub creator: BoundedText,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPackProfile {
    pub title: BoundedText,
    pub description: Option<BoundedText>,
    pub genre: Vec<BoundedText>,
    pub theme: Vec<BoundedText>,
    pub tone: Vec<BoundedText>,
    pub point_of_vew: BoundedText,
    pub tense: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleProfile {
    pub name: BoundedText,
    #[serde(default)]
    pub alias: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval_hint: BoundedText,
    pub default_profile: CharacterProfile,
    pub seed_memories: Vec<MemoryBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub play_role: RoleId,
}
