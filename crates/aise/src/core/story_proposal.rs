use crate::domain::ids::CharacterId;
use crate::domain::memory::MemoryKind;
use crate::domain::narrative::EventKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryProposal {
    pub story_text: String,
    pub events: Vec<ProposedEvent>,
    pub character_changes: Vec<ProposedCharacterChange>,
    pub world_change: ProposedWorldChange,
    pub memory_changes: Vec<ProposedMemoryChange>,
    pub summary_delta: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedEvent {
    pub kind: EventKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedCharacterChange {
    pub character_id: CharacterId,
    pub goal_updates: Vec<String>,
    pub health_delta: Option<i32>,
    pub affinity_deltas: Vec<ProposedAffinityDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAffinityDelta {
    pub other: CharacterId,
    pub delta: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProposedWorldChange {
    pub add_facts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedMemoryChange {
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
}
