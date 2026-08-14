use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{MemoryKind, TopicKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, MemoryId};
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub created_at_ms: i64,
}
