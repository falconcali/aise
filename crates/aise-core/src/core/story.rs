use super::{CharacterId, RoleId, StoryId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryOpeningInfo {
    pub sequence: u64,
    pub story_text: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryTurnInfo {
    pub turn_number: u64,
    pub sequence: u64,
    pub story_text: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleStateInfo {
    pub role_id: RoleId,
    pub name: String,
    pub source_character_id: Option<CharacterId>,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySnapshotInfo {
    pub story_id: StoryId,
    pub base_revision: u64,
    pub player_role_id: RoleId,
    pub opening: Option<StoryOpeningInfo>,
    pub roles: Vec<RoleStateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryHistoryInfo {
    pub opening: Option<StoryOpeningInfo>,
    pub turns: Vec<StoryTurnInfo>,
    pub next_turn_after: Option<u64>,
}
