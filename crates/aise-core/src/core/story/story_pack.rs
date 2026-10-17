use crate::core::role::validate_seed_roles;
use crate::core::{
    BoundedText, CoreError, FactBody, FactId, NarrativeError, NarrativeGraph, PackId, RoleId, RumorBody, RumorId,
    SeedRole, SemanticVersion, Sha256Digest, StoryError,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const STORY_PACK_SUBJECT: &str = "story_pack";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StoryPackSpec,
    pub pack_id: PackId,
    pub meta: StoryPackMeta,
    pub profile: StoryPackProfile,
    pub roles: BTreeMap<RoleId, SeedRole>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRef {
    pub pack_id: PackId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

impl StoryPack {
    pub fn validate_play(&self) -> Result<(), StoryError> {
        validate_play(&self.play, &self.roles)
    }

    pub fn validate_roles(&self) -> Result<(), StoryError> {
        validate_seed_roles(&self.roles)
    }

    pub fn validate_narrative(&self) -> Result<(), NarrativeError> {
        match &self.narrative {
            Some(graph) => graph.validate(),
            None => Ok(()),
        }
    }

    pub fn pack_ref(&self) -> Result<PackRef, CoreError> {
        Ok(PackRef {
            pack_id: self.pack_id.clone(),
            version: self.meta.version.clone(),
            digest: Sha256Digest::canonical(STORY_PACK_SUBJECT, self)?,
        })
    }
}

fn validate_play(play: &PlayDefinition, roles: &BTreeMap<RoleId, SeedRole>) -> Result<(), StoryError> {
    if play.playable_roles.is_empty() {
        return Err(StoryError::NoPlayableRole);
    }
    match play.playable_roles.iter().find(|role_id| !roles.contains_key(*role_id)) {
        Some(role_id) => Err(StoryError::UnknownRole {
            field: "play.playable_roles",
            role_id: role_id.to_string(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "tests/story_pack_tests.rs"]
mod tests;
