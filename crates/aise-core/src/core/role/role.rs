use crate::core::knowledge::RetrievalHint;
use crate::core::role::{CharacterProfile, MemoryBody, MemoryEntry, RoleActor};
use crate::core::{BoundedText, MemoryId, RoleId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBody {
    pub name: BoundedText,
    #[serde(default)]
    pub alias: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval: RetrievalHint,
    pub default_profile: CharacterProfile,
    #[serde(default)]
    pub seed_memories: BTreeMap<MemoryId, MemoryBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub id: RoleId,
    pub actor: RoleActor,
    pub memories: BTreeMap<MemoryId, MemoryEntry>,
}
