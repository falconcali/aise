use crate::domain::ids::{CharacterId, FactId};
use crate::domain::memory::MemoryKind;
use crate::domain::narrative::{EventKind, StorySummary};
use crate::domain::story_state::CurrentScene;
use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryProposal {
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub story_text: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub events: Vec<ProposedEvent>,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub character_changes: Vec<ProposedCharacterChange>,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub world_change: ProposedWorldChange,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub memory_changes: Vec<ProposedMemoryChange>,
    #[serde(default)]
    pub scene_change: Option<CurrentScene>,
    #[serde(default)]
    pub constraint_changes: Vec<String>,
    #[serde(default)]
    pub summary_change: Option<StorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedEvent {
    pub kind: EventKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedCharacterChange {
    pub character_id: CharacterId,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub goal_updates: Vec<String>,
    #[serde(default)]
    pub health_delta: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub affinity_deltas: Vec<ProposedAffinityDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAffinityDelta {
    pub other: CharacterId,
    pub delta: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProposedWorldChange {
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub add_facts: Vec<ProposedWorldFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedWorldFact {
    pub text: String,
    pub evidence: Vec<WorldFactEvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldFactEvidenceRef {
    SnapshotFact(FactId),
    ProposedEvent { event_index: u32 },
}

impl WorldFactEvidenceRef {
    pub fn as_str(&self) -> String {
        match self {
            WorldFactEvidenceRef::SnapshotFact(fact_id) => format!("snapshot_fact:{}", fact_id.as_str()),
            WorldFactEvidenceRef::ProposedEvent { event_index } => format!("proposed_event:{event_index}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedMemoryChange {
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
}
