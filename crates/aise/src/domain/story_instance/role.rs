use crate::domain::asset::character_card::CharacterProfile;
use crate::domain::asset::frozen_ref::FrozenCharacterCardRef;
use crate::domain::asset::ids::{AttributeKey, LocationKey, PlayerId};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{CharacterId, RoleId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "player_id", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleController {
    Player(PlayerId),
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRoleState {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRole {
    pub role_id: RoleId,
    pub controller: RoleController,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub effective_profile: CharacterProfile,
    pub source_character: Option<FrozenCharacterCardRef>,
    pub state: StoryRoleState,
}

impl StoryRole {
    pub fn is_player_controlled(&self) -> bool {
        matches!(self.controller, RoleController::Player(_))
    }

    pub fn compact_byte_len(&self) -> Result<usize, serde_json::Error> {
        Ok(serde_json::to_vec(self)?.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoryRoleView {
    pub role_id: RoleId,
    pub controller: RoleController,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub effective_profile: CharacterProfile,
    pub source_character_id: Option<CharacterId>,
    pub state: StoryRoleState,
}

impl StoryRoleView {
    pub fn is_player_controlled(&self) -> bool {
        matches!(self.controller, RoleController::Player(_))
    }
}

impl From<&StoryRole> for StoryRoleView {
    fn from(role: &StoryRole) -> Self {
        Self {
            role_id: role.role_id.clone(),
            controller: role.controller.clone(),
            role_label: role.role_label.clone(),
            narrative_function: role.narrative_function.clone(),
            background: role.background.clone(),
            effective_profile: role.effective_profile.clone(),
            source_character_id: role.source_character.as_ref().map(|card| card.character_id.clone()),
            state: role.state.clone(),
        }
    }
}

#[cfg(test)]
#[path = "tests/role_tests.rs"]
mod tests;
