use super::*;
use crate::core::NarrativeNodeId;
use serde_json::json;

fn graph() -> NarrativeGraph {
    serde_json::from_value(json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "dramatic_focus": "The traveler reaches the grey forest",
            "activate_when": { "type": "story_started" },
            "complete_when": { "type": "semantic", "condition_id": "identified", "criterion": "visitor identified" }
        },
        "reveal": {
            "title": "Reveal",
            "activate_when": { "type": "node_is", "node_id": "arrival", "status": "completed" },
            "complete_when": { "type": "turn_reaches", "turn": 10 },
            "skip_when": { "type": "semantic", "condition_id": "left", "criterion": "traveler left" },
            "constraints": [
                { "forbid": "the ritual truth is revealed", "window": "before_active" },
                { "forbid": "the traveler rests safely", "window": "while_active" }
            ]
        }
    }}))
    .expect("test graph should deserialize")
}

fn state(entries: &[(&str, NodeStatus)]) -> NarrativeState {
    let mut state = NarrativeState::default();
    for (id, status) in entries {
        state
            .node_status
            .insert(NarrativeNodeId::try_new(*id).expect("test node id should be valid"), *status);
    }
    state
}

fn query_ids(plan: &NarrativePlan) -> Vec<&str> {
    plan.queries.iter().map(|query| query.condition_id.as_str()).collect()
}

fn forbidden(plan: &NarrativePlan) -> Vec<&str> {
    plan.constraints.iter().map(|constraint| constraint.forbid.as_str()).collect()
}

#[test]
fn initial_plan_has_no_direction_and_no_semantic_query() {
    let plan = graph().plan(&NarrativeState::default()).expect("plan should succeed");

    assert!(plan.directions.is_empty());
    assert!(plan.queries.is_empty());
}

#[test]
fn active_node_contributes_direction_and_completion_query() {
    let plan = graph()
        .plan(&state(&[("arrival", NodeStatus::Active)]))
        .expect("plan should succeed");

    assert_eq!(plan.directions.len(), 1);
    assert_eq!(plan.directions[0].node_id.as_str(), "arrival");
    assert_eq!(
        plan.directions[0].dramatic_focus.as_str(),
        "The traveler reaches the grey forest"
    );
    assert_eq!(query_ids(&plan), ["identified"]);
}

#[test]
fn active_node_without_focus_still_contributes_skip_query() {
    let plan = graph()
        .plan(&state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Active)]))
        .expect("plan should succeed");

    assert!(plan.directions.is_empty());
    assert_eq!(query_ids(&plan), ["left"]);
}

#[test]
fn finished_nodes_contribute_nothing() {
    let plan = graph()
        .plan(&state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Skipped)]))
        .expect("plan should succeed");

    assert!(plan.directions.is_empty());
    assert!(plan.constraints.is_empty());
    assert!(plan.queries.is_empty());
}

#[test]
fn before_active_constraint_guards_until_its_node_activates() {
    let plan = graph()
        .plan(&state(&[("arrival", NodeStatus::Active)]))
        .expect("plan should succeed");

    assert_eq!(plan.constraints.len(), 1);
    assert_eq!(plan.constraints[0].node_id.as_str(), "reveal");
    assert_eq!(forbidden(&plan), ["the ritual truth is revealed"]);
}

#[test]
fn while_active_constraint_replaces_before_active_once_node_activates() {
    let plan = graph()
        .plan(&state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Active)]))
        .expect("plan should succeed");

    assert_eq!(forbidden(&plan), ["the traveler rests safely"]);
}
