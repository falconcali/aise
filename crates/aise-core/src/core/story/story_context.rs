use crate::core::{
    BoundedText, FactEntry, FactId, NarrativeState, PackRef, RoleEntry, RoleId, RumorEntry, RumorId, StoryId,
    StoryLifeCycle, Turn, TurnNumber,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
    pub covered_through: TurnNumber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryContext {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub opening: Arc<BoundedText>,
    pub roles: BTreeMap<RoleId, RoleEntry>,
    pub facts: BTreeMap<FactId, FactEntry>,
    pub rumors: BTreeMap<RumorId, RumorEntry>,
    pub player: RoleId,
    pub turn_number: TurnNumber,
    pub summary: Option<StorySummary>,
    pub recent_turns: VecDeque<Turn>,
    pub life_cycle: StoryLifeCycle,
    pub narrative: NarrativeState,
}
