use crate::domain::asset::ids::NarrativeNodeKey;
use crate::domain::ids::TurnNumber;
use crate::domain::narrative_graph::condition::NarrativeNodeState;
use crate::domain::narrative_graph::effect::{NarrativeEffectDefinition, NarrativeEffectId, NarrativeTransitionKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingNarrativeEffect {
    pub effect_id: NarrativeEffectId,
    pub source_node: NarrativeNodeKey,
    pub source_transition: NarrativeTransitionKind,
    pub source_graph_revision: u64,
    pub created_by_turn: Option<TurnNumber>,
    pub effect_index: u32,
    pub expires_after_turn: Option<u64>,
    pub definition: NarrativeEffectDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeRuntimeState {
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
    pub activation_turns: BTreeMap<NarrativeNodeKey, TurnNumber>,
    pub pending_effects: BTreeMap<NarrativeEffectId, PendingNarrativeEffect>,
}

impl NarrativeRuntimeState {
    pub fn initial() -> Self {
        Self {
            graph_revision: 0,
            node_states: BTreeMap::new(),
            activation_turns: BTreeMap::new(),
            pending_effects: BTreeMap::new(),
        }
    }

    pub fn node_state(&self, node: &NarrativeNodeKey) -> NarrativeNodeState {
        self.node_states.get(node).copied().unwrap_or(NarrativeNodeState::Inactive)
    }
}

#[cfg(test)]
#[path = "tests/state_tests.rs"]
mod tests;
