use crate::core::activation::ActivationRule;
use crate::core::knowledge::{Recallable, RetrievalHint};
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
    #[serde(default)]
    pub activation: ActivationRule,
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

impl Recallable for RoleBody {
    fn content(&self) -> &BoundedText {
        &self.backstory
    }

    fn activation(&self) -> &ActivationRule {
        &self.activation
    }

    fn retrieval(&self) -> &RetrievalHint {
        &self.retrieval
    }
}
