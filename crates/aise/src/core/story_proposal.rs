use crate::domain::ids::CharacterId;
use crate::domain::memory::MemoryKind;
use crate::domain::narrative::EventKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryProposal {
    #[serde(default)]
    pub story_text: String,
    #[serde(default)]
    pub events: Vec<ProposedEvent>,
    #[serde(default)]
    pub character_changes: Vec<ProposedCharacterChange>,
    #[serde(default)]
    pub world_change: ProposedWorldChange,
    #[serde(default)]
    pub memory_changes: Vec<ProposedMemoryChange>,
    #[serde(default)]
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
    #[serde(default)]
    pub goal_updates: Vec<String>,
    #[serde(default)]
    pub health_delta: Option<i32>,
    #[serde(default)]
    pub affinity_deltas: Vec<ProposedAffinityDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAffinityDelta {
    pub other: CharacterId,
    pub delta: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProposedWorldChange {
    #[serde(default)]
    pub add_facts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedMemoryChange {
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
}
