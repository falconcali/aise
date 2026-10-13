use crate::core::{
    NarrativeState, PackId, PackRef, RoleActor, RoleId, SemanticVersion, Sha256Digest, StoryId, StoryLifeCycle, Turn,
    TurnNumber, WorldState,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
    pub covered_through: TurnNumber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryContext {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub cast: BTreeMap<RoleId, RoleActor>,
    pub player: RoleId,
    pub turn_number: TurnNumber,
    pub summary: Option<StorySummary>,
    pub recent_turns: VecDeque<Turn>,
    pub life_cycle: StoryLifeCycle,
    pub world_state: WorldState,
    pub narrative: NarrativeState,
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
            narrative: NarrativeState::default(),
        }
    }
}
