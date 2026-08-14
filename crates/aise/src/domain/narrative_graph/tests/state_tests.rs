use crate::domain::asset::ids::NarrativeNodeKey;
use crate::domain::narrative_graph::condition::NarrativeNodeState;
use crate::domain::narrative_graph::state::NarrativeRuntimeState;

#[test]
fn initial_state_has_no_active_nodes_or_pending_effects() {
    let state = NarrativeRuntimeState::initial();
    assert_eq!(state.graph_revision, 0);
    assert!(state.node_states.is_empty());
    assert!(state.pending_effects.is_empty());
}

#[test]
fn node_state_defaults_to_inactive_when_absent() {
    let state = NarrativeRuntimeState::initial();
    let node_key = NarrativeNodeKey::try_new("node.unknown").unwrap();
    assert_eq!(state.node_state(&node_key), NarrativeNodeState::Inactive);
}

#[test]
fn node_state_reflects_explicit_entry() {
    let mut state = NarrativeRuntimeState::initial();
    let node_key = NarrativeNodeKey::try_new("node.intro").unwrap();
    state.node_states.insert(node_key.clone(), NarrativeNodeState::Completed);
    assert_eq!(state.node_state(&node_key), NarrativeNodeState::Completed);
}
