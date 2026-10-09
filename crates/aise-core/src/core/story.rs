use super::{
    Change, CharacterId, PackId, PlayerId, RoleId, SemanticVersion, Sha256Digest, StoryId, Turn, TurnNumber, WorldState,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRef {
    pub pack_id: PackId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardRef {
    pub character_id: CharacterId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct StoryInstanceSpec {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, CharacterCardRef>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInstanceInfo {
    pub story_id: StoryId,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub life_cycle: StoryLifeCycle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
    pub covered_through: TurnNumber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoryLifeCycle {
    Active,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryCommit {
    pub story_id: StoryId,
    pub turn: Turn,
    pub summary: Change<StorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryContext {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, CharacterCardRef>,
    pub player: RoleId,
    pub turn_number: TurnNumber,
    pub summary: Option<StorySummary>,
    pub recent_turns: VecDeque<Turn>,
    pub life_cycle: StoryLifeCycle,
    pub world_state: WorldState,
}

impl StoryContext {
    pub fn new() -> Self {
        Self {
            story_id: StoryId::try_new("default-story").expect("valid default story id"),
            pack_ref: PackRef {
                pack_id: PackId::try_new("default-pack").expect("valid default pack id"),
                version: SemanticVersion::try_new("0.0.0").expect("valid default version"),
                digest: Sha256Digest::try_new("default-digest").expect("valid default digest"),
            },
            cast: BTreeMap::new(),
            player: RoleId::try_new("player").expect("valid default role id"),
            turn_number: TurnNumber::new(0),
            summary: None,
            recent_turns: VecDeque::new(),
            life_cycle: StoryLifeCycle::Active,
            world_state: WorldState {},
        }
    }
}
