pub use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{PackId, Sha256Digest};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, EventId, FactId, MemoryId, RumorId, StoryRevision, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeIndexMatch {
    Entity(KnowledgeEntity),
    Topic(crate::domain::asset::ids::TopicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Fact,
    Rumor,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSource {
    Seed { pack_id: PackId, pack_digest: Sha256Digest },
    CommittedTurn { turn_id: TurnId, event_id: Option<EventId> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum KnowledgeSourceId {
    Fact(FactId),
    Rumor(RumorId),
    Memory(MemoryId),
}

impl KnowledgeSourceId {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Fact(id) => id.as_str(),
            Self::Rumor(id) => id.as_str(),
            Self::Memory(id) => id.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentPerception {
    pub character_id: CharacterId,
    pub source_event_id: EventId,
    pub content: BoundedText,
    pub story_revision: StoryRevision,
}
