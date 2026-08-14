use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::KnowledgeKind;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RetrievalTargetId(Arc<str>);

impl RetrievalTargetId {
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, &'static str> {
        let value = value.as_ref();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_whitespace) {
            return Err("invalid retrieval target id");
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn for_character(character_id: &CharacterId) -> Self {
        Self(Arc::from(format!("character:{}", character_id.as_str())))
    }

    pub fn for_knowledge(source_id: &crate::domain::knowledge::KnowledgeSourceId) -> Self {
        let kind = match source_id {
            crate::domain::knowledge::KnowledgeSourceId::Fact(_) => "fact",
            crate::domain::knowledge::KnowledgeSourceId::Rumor(_) => "rumor",
            crate::domain::knowledge::KnowledgeSourceId::Memory(_) => "memory",
        };
        Self(Arc::from(format!("knowledge:{kind}:{}", source_id.as_str())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalIndexScope {
    Complete,
    Prefiltered,
}

impl std::fmt::Display for RetrievalTargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RetrievalTargetId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RetrievalTargetId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RetrievalAudience {
    GlobalWriter,
    Character { character_id: CharacterId },
}

impl Serialize for RetrievalAudience {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(match self {
            Self::GlobalWriter => 1,
            Self::Character { .. } => 2,
        }))?;
        match self {
            Self::GlobalWriter => {
                map.serialize_entry("kind", "global_writer")?;
            }
            Self::Character { character_id } => {
                map.serialize_entry("kind", "character")?;
                map.serialize_entry("character_id", character_id)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for RetrievalAudience {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind = object
            .remove("kind")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("audience kind is required"))?;
        match kind.as_str() {
            "global_writer" if object.is_empty() => Ok(Self::GlobalWriter),
            "character" if object.len() == 1 => {
                let character_id = object
                    .remove("character_id")
                    .and_then(|value| value.as_str().map(CharacterId::from))
                    .ok_or_else(|| serde::de::Error::custom("character_id is required"))?;
                Ok(Self::Character { character_id })
            }
            _ => Err(serde::de::Error::custom("invalid retrieval audience shape")),
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
pub struct RetrievalRequest {
    pub audience: RetrievalAudience,
    pub target_source_id: Option<crate::domain::knowledge::KnowledgeSourceId>,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub authorized_memory_owners: Vec<CharacterId>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalPlan {
    pub requests: Vec<RetrievalRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThinkRequest {
    pub character_id: CharacterId,
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
