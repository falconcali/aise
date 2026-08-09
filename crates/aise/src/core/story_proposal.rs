use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, FactId};
use crate::domain::memory::MemoryKind;
use crate::domain::narrative::{EventKind, StorySummary};
use crate::domain::story_instance::state::CurrentScene;
use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn deserialize_summary_change<'de, D>(deserializer: D) -> Result<Option<StorySummary>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SummaryChangeRepr {
        Summary(StorySummary),
        PlainText(String),
    }

    Ok(match Option::<SummaryChangeRepr>::deserialize(deserializer)? {
        None => None,
        Some(SummaryChangeRepr::Summary(summary)) => Some(summary),
        Some(SummaryChangeRepr::PlainText(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(StorySummary {
                    text: BoundedText::try_new(trimmed.to_owned(), "summary", usize::MAX)
                        .map_err(serde::de::Error::custom)?,
                    summarized_through: None,
                })
            }
        }
    })
}

fn deserialize_add_facts<'de, D>(deserializer: D) -> Result<Vec<ProposedWorldFact>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum AddFactsRepr {
        Facts(Vec<ProposedWorldFact>),
        PlainText(Vec<String>),
    }

    Ok(match Option::<AddFactsRepr>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(AddFactsRepr::Facts(facts)) => facts,
        Some(AddFactsRepr::PlainText(texts)) => texts
            .into_iter()
            .map(|text| ProposedWorldFact {
                text,
                evidence: Vec::new(),
            })
            .collect(),
    })
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
    #[serde(default, deserialize_with = "deserialize_summary_change")]
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
    #[serde(default, deserialize_with = "deserialize_add_facts")]
    pub add_facts: Vec<ProposedWorldFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedWorldFact {
    pub text: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
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
