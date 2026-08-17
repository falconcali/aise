use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalIndexScope {
    Complete,
    Prefiltered,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KnowledgeDelivery {
    Writer,
    Character { role_id: RoleId },
}

impl Serialize for KnowledgeDelivery {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(match self {
            Self::Writer => 1,
            Self::Character { .. } => 2,
        }))?;
        match self {
            Self::Writer => {
                map.serialize_entry("kind", "writer")?;
            }
            Self::Character { role_id } => {
                map.serialize_entry("kind", "character")?;
                map.serialize_entry("role_id", role_id)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for KnowledgeDelivery {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind = object
            .remove("kind")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("delivery kind is required"))?;
        match kind.as_str() {
            "writer" if object.is_empty() => Ok(Self::Writer),
            "character" if object.len() == 1 => {
                let role_id = object
                    .remove("role_id")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or_else(|| serde::de::Error::custom("role_id is required"))?;
                let role_id = RoleId::try_new(role_id).map_err(|_| serde::de::Error::custom("role_id is invalid"))?;
                Ok(Self::Character { role_id })
            }
            _ => Err(serde::de::Error::custom("invalid knowledge delivery shape")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalRequestOrigin {
    Automatic,
    Narrative,
    Planner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterRetrievalRequest {
    pub role_id: RoleId,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeRetrievalRequest {
    pub delivery: KnowledgeDelivery,
    pub target_source_id: Option<KnowledgeSourceId>,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalPlan {
    pub character_requests: Vec<CharacterRetrievalRequest>,
    pub knowledge_requests: Vec<KnowledgeRetrievalRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThinkRequest {
    pub role_id: RoleId,
    pub reason: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterStoryGoal {
    pub summary: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriterPlan {
    pub story_goal: WriterStoryGoal,
    pub retrieval_plan: RetrievalPlan,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}
