use crate::domain::asset::ids::MemoryKind;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{MemoryId, RoleId};
use crate::domain::knowledge::query::KnowledgeSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: RoleId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub created_at_ms: i64,
}
