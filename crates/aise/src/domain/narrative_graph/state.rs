use crate::core::turn_contract::TurnId;
use crate::domain::asset::ids::NarrativeNodeKey;
use crate::domain::narrative_graph::definition::NarrativeNodeState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeRuntimeState {
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
    pub activation_turns: BTreeMap<NarrativeNodeKey, TurnId>,
}

impl NarrativeRuntimeState {
    pub fn initial() -> Self {
        Self {
            graph_revision: 0,
            node_states: BTreeMap::new(),
            activation_turns: BTreeMap::new(),
        }
    }

    pub fn node_state(&self, node: &NarrativeNodeKey) -> NarrativeNodeState {
        self.node_states.get(node).copied().unwrap_or(NarrativeNodeState::Inactive)
    }
}
