use crate::core::turn_contract::StoryRevision;
use crate::domain::asset::ids::{MemoryKind, TopicKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, MemoryId};
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
    pub created_at_ms: i64,
}
