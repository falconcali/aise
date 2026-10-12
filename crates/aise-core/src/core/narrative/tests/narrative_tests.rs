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

    assert!(graph.validate(&NarrativeConfig::default()).is_ok());
}

#[test]
fn node_reference_to_missing_node_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(json!({ "type": "node_is", "node_id": "ghost", "status": "active" }), started()),
    }}));

    assert!(matches!(
        graph.validate(&NarrativeConfig::default()),
        Err(NarrativeError::UnknownNode { node_id }) if node_id == "ghost"
    ));
}

#[test]
fn empty_all_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(json!({ "type": "all", "conditions": [] }), started()),
    }}));

    assert!(matches!(
        graph.validate(&NarrativeConfig::default()),
        Err(NarrativeError::EmptyCondition { operator: "all" })
    ));
}

#[test]
fn condition_deeper_than_limit_is_rejected() {
    let nested = json!({ "type": "not", "condition": { "type": "not", "condition": started() } });
    let graph = graph(json!({ "nodes": { "a": node(nested, started()) } }));
    let config = NarrativeConfig {
        max_condition_depth: 2,
        ..NarrativeConfig::default()
    };

    assert!(matches!(
        graph.validate(&config),
        Err(NarrativeError::ConditionTooDeep { maximum: 2 })
    ));
}

#[test]
fn same_semantic_id_with_different_criteria_is_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("done", "first")),
        "b": node(started(), semantic("done", "second")),
    }}));

    assert!(matches!(
        graph.validate(&NarrativeConfig::default()),
        Err(NarrativeError::SemanticCriterionConflict { condition_id }) if condition_id == "done"
    ));
}

#[test]
fn same_semantic_id_with_same_criterion_is_accepted() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("done", "same")),
        "b": node(started(), semantic("done", "same")),
    }}));

    assert!(graph.validate(&NarrativeConfig::default()).is_ok());
}

#[test]
fn too_many_nodes_are_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), started()),
        "b": node(started(), started()),
    }}));
    let config = NarrativeConfig {
        max_nodes: 1,
        ..NarrativeConfig::default()
    };

    assert!(matches!(
        graph.validate(&config),
        Err(NarrativeError::TooManyNodes { actual: 2, maximum: 1 })
    ));
}

#[test]
fn too_many_semantic_conditions_are_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": node(started(), semantic("one", "one")),
        "b": node(started(), semantic("two", "two")),
    }}));
    let config = NarrativeConfig {
        max_semantic_conditions: 1,
        ..NarrativeConfig::default()
    };

    assert!(matches!(
        graph.validate(&config),
        Err(NarrativeError::TooManySemanticConditions { actual: 2, maximum: 1 })
    ));
}

#[test]
fn too_many_constraints_on_a_node_are_rejected() {
    let graph = graph(json!({ "nodes": {
        "a": {
            "title": "node",
            "activate_when": started(),
            "complete_when": started(),
            "constraints": [
                { "forbid": "one", "window": "before_active" },
                { "forbid": "two", "window": "while_active" }
            ]
        }
    }}));
    let config = NarrativeConfig {
        max_constraints_per_node: 1,
        ..NarrativeConfig::default()
    };

    assert!(matches!(
        graph.validate(&config),
        Err(NarrativeError::TooManyConstraints { node_id, actual: 2, maximum: 1 }) if node_id == "a"
    ));
}
