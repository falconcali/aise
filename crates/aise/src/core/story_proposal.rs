use crate::domain::ids::CharacterId;
use crate::domain::memory::MemoryKind;
use crate::domain::narrative::EventKind;
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
    pub add_facts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedMemoryChange {
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
}
