use crate::core::{FactBody, FactId, MemoryBody, MemoryId, RoleId, RumorBody, RumorId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeChange {
    #[serde(default)]
    pub facts: Vec<FactChangeOp>,
    #[serde(default)]
    pub rumors: Vec<RumorChangeOp>,
    #[serde(default)]
    pub memories: Vec<RoleMemoryChangeOp>,
}

impl KnowledgeChange {
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.rumors.is_empty() && self.memories.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactChangeOp {
    Add { id: FactId, body: FactBody },
    Update { id: FactId, body: FactBody },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RumorChangeOp {
    Add { id: RumorId, body: RumorBody },
    Update { id: RumorId, body: RumorBody },
    Remove { id: RumorId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleMemoryChangeOp {
    Add {
        role_id: RoleId,
        id: MemoryId,
        body: MemoryBody,
    },
    Update {
        role_id: RoleId,
        id: MemoryId,
        body: MemoryBody,
    },
}

#[cfg(test)]
#[path = "tests/change_tests.rs"]
mod tests;
