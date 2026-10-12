use crate::core::asset::AssetError;
use crate::core::role::{RoleActor, RoleBody};
use crate::core::{BoundedText, FactBody, FactId, NarrativeGraph, RoleId, RumorBody, RumorId, SemanticVersion};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StoryPackSpec,
    pub meta: StoryPackMeta,
    pub profile: StoryPackProfile,
    pub roles: BTreeMap<RoleId, RoleBody>,
    pub play: PlayDefinition,
    pub opening: BoundedText,
    #[serde(default)]
    pub seed_facts: BTreeMap<FactId, FactBody>,
    #[serde(default)]
    pub seed_rumors: BTreeMap<RumorId, RumorBody>,
    #[serde(default)]
    pub constraints: Vec<BoundedText>,
    #[serde(default)]
    pub narrative: Option<NarrativeGraph>,
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
    #[serde(default)]
    pub genres: Vec<BoundedText>,
    #[serde(default)]
    pub themes: Vec<BoundedText>,
    #[serde(default)]
    pub tones: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub playable_roles: BTreeSet<RoleId>,
}

impl StoryPack {
    pub fn resolve_cast(
        &self,
        player_role: &RoleId,
        mut cast: BTreeMap<RoleId, RoleActor>,
    ) -> Result<BTreeMap<RoleId, RoleActor>, AssetError> {
        if !self.play.playable_roles.contains(player_role) {
            return Err(AssetError::RoleNotPlayable {
                role_id: player_role.to_string(),
            });
        }
        if let Some(role_id) = cast.keys().find(|role_id| !self.roles.contains_key(*role_id)) {
            return Err(AssetError::UnknownRole {
                field: "cast",
                role_id: role_id.to_string(),
            });
        }
        for role_id in self.roles.keys() {
            cast.entry(role_id.clone()).or_insert(RoleActor::Default);
        }
        Ok(cast)
    }
}

#[cfg(test)]
#[path = "tests/story_pack_tests.rs"]
mod tests;
