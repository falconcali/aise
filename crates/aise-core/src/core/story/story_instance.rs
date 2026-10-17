use crate::core::{
    FactEntry, FactId, NarrativeState, PackRef, PlayerId, RoleEntry, RoleId, RumorEntry, RumorId, StoryId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct StoryInstanceSpec {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub roles: BTreeMap<RoleId, RoleEntry>,
    pub facts: BTreeMap<FactId, FactEntry>,
    pub rumors: BTreeMap<RumorId, RumorEntry>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
    pub narrative: NarrativeState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInstanceInfo {
    pub story_id: StoryId,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub life_cycle: StoryLifeCycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryLifeCycle {
    Active,
    Cancelled,
    Completed,
}
