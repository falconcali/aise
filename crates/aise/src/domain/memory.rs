use serde::{Deserialize, Serialize};

use super::ids::{CharacterId, MemoryId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
    /// Unix milliseconds.
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Observed,
    Inferred,
    Secret,
}

/// A requested memory write, produced by a story draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPatch {
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: String,
}
