use super::*;
use serde_json::json;

fn constraint(window: &str) -> NarrativeConstraint {
    serde_json::from_value(json!({ "forbid": "the ritual truth is revealed", "window": window }))
        .expect("test constraint should deserialize")
}

#[test]
fn before_active_applies_only_while_node_is_inactive() {
    let constraint = constraint("before_active");

    assert!(constraint.applies(NodeStatus::Inactive));
    assert!(!constraint.applies(NodeStatus::Active));
    assert!(!constraint.applies(NodeStatus::Completed));
    assert!(!constraint.applies(NodeStatus::Skipped));
}

#[test]
fn while_active_applies_only_while_node_is_active() {
    let constraint = constraint("while_active");

    assert!(!constraint.applies(NodeStatus::Inactive));
    assert!(constraint.applies(NodeStatus::Active));
    assert!(!constraint.applies(NodeStatus::Completed));
    assert!(!constraint.applies(NodeStatus::Skipped));
}

#[test]
fn constraint_without_window_is_rejected() {
    let result = serde_json::from_value::<NarrativeConstraint>(json!({ "forbid": "anything" }));

    assert!(result.is_err());
}
