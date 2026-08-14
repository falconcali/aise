pub use crate::domain::asset::entity::KnowledgeEntity;

use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::fact::WorldFact;
use crate::domain::knowledge::memory::MemoryEntry;
use crate::domain::knowledge::query::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::domain::knowledge::rumor::SharedRumor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeEntry {
    Fact(WorldFact),
    Rumor(SharedRumor),
    Memory(MemoryEntry),
}

impl KnowledgeEntry {
    pub fn source_id(&self) -> KnowledgeSourceId {
        match self {
            Self::Fact(value) => KnowledgeSourceId::Fact(value.id.clone()),
            Self::Rumor(value) => KnowledgeSourceId::Rumor(value.id.clone()),
            Self::Memory(value) => KnowledgeSourceId::Memory(value.id.clone()),
        }
    }

    pub fn kind(&self) -> KnowledgeKind {
        match self {
            Self::Fact(_) => KnowledgeKind::Fact,
            Self::Rumor(_) => KnowledgeKind::Rumor,
            Self::Memory(_) => KnowledgeKind::Memory,
        }
    }

    pub fn content(&self) -> &BoundedText {
        match self {
            Self::Fact(value) => &value.text,
            Self::Rumor(value) => &value.content,
            Self::Memory(value) => &value.content,
        }
    }

    pub fn entities(&self) -> &[KnowledgeEntity] {
        match self {
            Self::Fact(value) => &value.entities,
            Self::Rumor(value) => &value.entities,
            Self::Memory(value) => &value.entities,
        }
    }

    pub fn topics(&self) -> &[TopicKey] {
        match self {
            Self::Fact(value) => &value.topics,
            Self::Rumor(value) => &value.topics,
            Self::Memory(value) => &value.topics,
        }
    }

    pub fn salience(&self) -> u8 {
        match self {
            Self::Fact(value) => value.salience,
            Self::Rumor(value) => value.salience,
            Self::Memory(value) => value.salience,
        }
    }

    pub fn source(&self) -> &KnowledgeSource {
        match self {
            Self::Fact(value) => &value.source,
            Self::Rumor(value) => &value.source,
            Self::Memory(value) => &value.source,
        }
    }

    pub fn memory_owner(&self) -> Option<&CharacterId> {
        match self {
            Self::Memory(value) => Some(&value.owner),
            _ => None,
        }
    }
}
