use super::*;
use serde_json::json;

fn graph(value: serde_json::Value) -> NarrativeGraph {
    serde_json::from_value(value).expect("test graph should deserialize")
}

fn node(activate_when: serde_json::Value, complete_when: serde_json::Value) -> serde_json::Value {
    json!({ "title": "node", "activate_when": activate_when, "complete_when": complete_when })
}

fn started() -> serde_json::Value {
    json!({ "type": "story_started" })
}

fn semantic(id: &str, criterion: &str) -> serde_json::Value {
    json!({ "type": "semantic", "condition_id": id, "criterion": criterion })
}

#[test]
fn valid_graph_passes_validation() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("done", "a is done")),
        "b": node(json!({ "type": "node_is", "node_id": "a", "status": "completed" }), started()),
    }}));

    assert!(graph.validate().is_ok());
}

#[test]
fn node_reference_to_missing_node_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(json!({ "type": "node_is", "node_id": "ghost", "status": "active" }), started()),
    }}));

    assert!(matches!(
        graph.validate(),
        Err(NarrativeError::UnknownNode { node_id }) if node_id == "ghost"
    ));
}

#[test]
fn empty_all_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(json!({ "type": "all", "conditions": [] }), started()),
    }}));

    assert!(matches!(
        graph.validate(),
        Err(NarrativeError::EmptyCondition { operator: "all" })
    ));
}

#[test]
fn same_semantic_id_with_different_criteria_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("done", "first")),
        "b": node(started(), semantic("done", "second")),
    }}));

    assert!(matches!(
        graph.validate(),
        Err(NarrativeError::SemanticCriterionConflict { condition_id }) if condition_id == "done"
    ));
}

#[test]
fn same_semantic_id_with_same_criterion_is_accepted() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("done", "same")),
        "b": node(started(), semantic("done", "same")),
    }}));

    assert!(graph.validate().is_ok());
}
