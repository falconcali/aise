use super::*;

fn node_id(value: &str) -> NarrativeNodeId {
    NarrativeNodeId::try_new(value).expect("test node id should be valid")
}

#[test]
fn unknown_node_is_inactive() {
    assert_eq!(NarrativeState::default().status(&node_id("arrival")), NodeStatus::Inactive);
}

#[test]
fn apply_overwrites_node_status() {
    let mut state = NarrativeState::default();
    state.apply(&[NarrativeTransition {
        node_id: node_id("arrival"),
        to: NodeStatus::Active,
    }]);
    state.apply(&[NarrativeTransition {
        node_id: node_id("arrival"),
        to: NodeStatus::Completed,
    }]);

    assert_eq!(state.status(&node_id("arrival")), NodeStatus::Completed);
}
