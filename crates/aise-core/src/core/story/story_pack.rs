use crate::core::{
    BoundedText, CharacterCard, CharacterCardRef, CoreError, FactBody, FactId, NarrativeError, NarrativeGraph, PackId,
    RoleActor, RoleBody, RoleId, RumorBody, RumorId, SemanticVersion, Sha256Digest, StoryError,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const STORY_PACK_SUBJECT: &str = "story_pack";
pub const MAX_ROLES_PER_PACK: usize = 64;
pub const MAX_ALIASES_PER_ROLE: usize = 8;
pub const MAX_ROLE_CATALOG_BYTES: usize = 32 * 1024;
pub const MAX_SEED_MEMORIES_PER_ROLE: usize = 32;
pub const MAX_ROLE_ASSETS_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StoryPackSpec,
    pub pack_id: PackId,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRef {
    pub pack_id: PackId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct StoryAssetSpec {
    pub pack_ref: PackRef,
    pub pack: StoryPack,
    pub cards: Vec<(CharacterCardRef, CharacterCard)>,
}

impl StoryPack {
    pub fn validate_play(&self) -> Result<(), StoryError> {
        validate_play(&self.play, &self.roles)
    }

    pub fn validate_roles(&self) -> Result<(), StoryError> {
        if self.roles.len() > MAX_ROLES_PER_PACK {
            return Err(StoryError::TooManyRoles {
                actual: self.roles.len(),
                maximum: MAX_ROLES_PER_PACK,
            });
        }
        let mut catalog_bytes = 0usize;
        for (role_id, role) in &self.roles {
            if role.alias.len() > MAX_ALIASES_PER_ROLE {
                return Err(StoryError::TooManyRoleAliases {
                    role_id: role_id.clone(),
                    actual: role.alias.len(),
                    maximum: MAX_ALIASES_PER_ROLE,
                });
            }
            if role.seed_memories.len() > MAX_SEED_MEMORIES_PER_ROLE {
                return Err(StoryError::TooManySeedMemories {
                    role_id: role_id.clone(),
                    actual: role.seed_memories.len(),
                    maximum: MAX_SEED_MEMORIES_PER_ROLE,
                });
            }
            catalog_bytes = catalog_bytes
                .saturating_add(role_id.as_str().len())
                .saturating_add(role.name.as_str().len())
                .saturating_add(
                    role.alias
                        .iter()
                        .fold(0usize, |total, alias| total.saturating_add(alias.as_str().len())),
                )
                .saturating_add(role.retrieval.as_str().len());
        }
        if catalog_bytes > MAX_ROLE_CATALOG_BYTES {
            return Err(StoryError::RoleCatalogTooLarge {
                actual: catalog_bytes,
                maximum: MAX_ROLE_CATALOG_BYTES,
            });
        }
        let role_assets = serde_json::to_vec(&self.roles).map_err(|error| StoryError::RoleAssetsSerialization {
            message: error.to_string(),
        })?;
        if role_assets.len() > MAX_ROLE_ASSETS_BYTES {
            return Err(StoryError::RoleAssetsTooLarge {
                actual: role_assets.len(),
                maximum: MAX_ROLE_ASSETS_BYTES,
            });
        }
        Ok(())
    }

    pub fn validate_narrative(&self) -> Result<(), NarrativeError> {
        match &self.narrative {
            Some(graph) => graph.validate(),
            None => Ok(()),
        }
    }

    pub fn resolve_cast(
        &self,
        player_role: &RoleId,
        mut cast: BTreeMap<RoleId, RoleActor>,
    ) -> Result<BTreeMap<RoleId, RoleActor>, StoryError> {
        if !self.play.playable_roles.contains(player_role) {
            return Err(StoryError::RoleNotPlayable {
                role_id: player_role.to_string(),
            });
        }
        if let Some(role_id) = cast.keys().find(|role_id| !self.roles.contains_key(*role_id)) {
            return Err(StoryError::UnknownRole {
                field: "cast",
                role_id: role_id.to_string(),
            });
        }
        for role_id in self.roles.keys() {
            cast.entry(role_id.clone()).or_insert(RoleActor::Default);
        }
        Ok(cast)
    }

    pub fn pack_ref(&self) -> Result<PackRef, CoreError> {
        Ok(PackRef {
            pack_id: self.pack_id.clone(),
            version: self.meta.version.clone(),
            digest: Sha256Digest::canonical(STORY_PACK_SUBJECT, self)?,
        })
    }
}

fn validate_play(play: &PlayDefinition, roles: &BTreeMap<RoleId, RoleBody>) -> Result<(), StoryError> {
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
