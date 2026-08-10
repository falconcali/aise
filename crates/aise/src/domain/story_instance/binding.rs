use crate::domain::asset::frozen_ref::FrozenCharacterAssetRef;
use crate::domain::asset::ids::{PackId, PlayerId, StoryRoleKey};
use crate::domain::ids::{CharacterId, StoryId, StoryRevision};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "player_id", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleController {
    Player(PlayerId),
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBinding {
    pub role_key: StoryRoleKey,
    pub character_id: CharacterId,
    pub character_asset: FrozenCharacterAssetRef,
    pub controller: RoleController,
    pub bound_at_ms: i64,
}

impl RoleBinding {
    pub fn is_player_controlled(&self) -> bool {
        matches!(self.controller, RoleController::Player(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInstanceBinding {
    pub story_id: StoryId,
    pub pack_id: PackId,
    pub revision: StoryRevision,
    pub role_bindings: Vec<RoleBinding>,
}

impl StoryInstanceBinding {
    pub fn character_id_for_role(&self, role_key: &StoryRoleKey) -> Option<&CharacterId> {
        self.role_bindings
            .iter()
            .find(|binding| &binding.role_key == role_key)
            .map(|binding| &binding.character_id)
    }
}
