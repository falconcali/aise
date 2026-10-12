use crate::core::TurnNumber;
use crate::core::narrative::{NarrativeEvidence, NarrativeGraph, NarrativeState, NarrativeTransition, NodeStatus};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize)]
pub struct NarrativeResolution {
    pub transitions: Vec<NarrativeTransition>,
    pub story_completed: bool,
}

impl NarrativeGraph {
    pub fn initial_state(&self) -> NarrativeState {
        let mut state = NarrativeState::default();
        let semantic_results = BTreeMap::new();
        let evidence = NarrativeEvidence {
            turn: TurnNumber::new(0),
            semantic_results: &semantic_results,
        };
        state.apply(&self.resolve(&state, &evidence).transitions);
        state
    }

    pub fn resolve(&self, state: &NarrativeState, evidence: &NarrativeEvidence<'_>) -> NarrativeResolution {
        let transitions: Vec<NarrativeTransition> = self
            .nodes
            .iter()
            .filter_map(|(node_id, node)| {
                node.next_status(state.status(node_id), state, evidence)
                    .map(|to| NarrativeTransition {
                        node_id: node_id.clone(),
                        to,
                    })
            })
            .collect();
        let story_completed = transitions.iter().any(|transition| self.completes_story(transition));
        NarrativeResolution {
            transitions,
            story_completed,
        }
    }

    fn completes_story(&self, transition: &NarrativeTransition) -> bool {
        transition.to == NodeStatus::Completed && self.nodes.get(&transition.node_id).is_some_and(|node| node.terminal)
    }
}

#[cfg(test)]
#[path = "tests/resolver_tests.rs"]
mod tests;
