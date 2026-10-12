use crate::core::activation::ActivationRule;
use crate::core::knowledge::RetrievalHint;
use crate::core::{BoundedText, MemoryId, RoleId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryBody {
    pub owner_id: RoleId,
    pub content: BoundedText,
    pub activation: ActivationRule,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry {
    pub memory_id: MemoryId,
    pub memory_item: MemoryBody,
    pub created_at_ms: u64,
}
