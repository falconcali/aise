use crate::core::NarrativeNodeId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Inactive,
    Active,
    Completed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NarrativeTransition {
    pub node_id: NarrativeNodeId,
    pub to: NodeStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeState {
    #[serde(default)]
    pub node_status: BTreeMap<NarrativeNodeId, NodeStatus>,
}

impl NarrativeState {
    pub fn status(&self, node_id: &NarrativeNodeId) -> NodeStatus {
        self.node_status.get(node_id).copied().unwrap_or(NodeStatus::Inactive)
    }

    pub fn apply(&mut self, transitions: &[NarrativeTransition]) {
        for transition in transitions {
            self.node_status.insert(transition.node_id.clone(), transition.to);
        }
    }
}

#[cfg(test)]
#[path = "tests/state_tests.rs"]
mod tests;
