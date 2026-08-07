use crate::core::turn_contract::{StoryId, StoryRevision};
use crate::domain::asset::ids::{PackId, PlayerId, StoryRoleKey};
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleBinding {
    pub role_key: StoryRoleKey,
    pub player_id: Option<PlayerId>,
    pub character_id: CharacterId,
    pub bound_at_ms: i64,
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
