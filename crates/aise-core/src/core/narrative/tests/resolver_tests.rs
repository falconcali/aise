use super::*;
use crate::core::{NarrativeConditionId, NarrativeNodeId, NodeStatus, Truth, TurnNumber};
use serde_json::json;
use std::collections::BTreeMap;

fn graph() -> NarrativeGraph {
    serde_json::from_value(json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "activate_when": { "type": "story_started" },
            "complete_when": { "type": "semantic", "condition_id": "identified", "criterion": "visitor identified" }
        },
        "reveal": {
            "title": "Reveal",
            "activate_when": { "type": "node_is", "node_id": "arrival", "status": "completed" },
            "complete_when": { "type": "semantic", "condition_id": "revealed", "criterion": "truth revealed" },
            "skip_when": { "type": "semantic", "condition_id": "left", "criterion": "traveler left" },
            "terminal": true
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

fn resolution(state: &NarrativeState, results: &[(&str, Truth)]) -> NarrativeResolution {
    let semantic_results: BTreeMap<NarrativeConditionId, Truth> = results
        .iter()
        .map(|(id, truth)| {
            (
                NarrativeConditionId::try_new(*id).expect("test condition id should be valid"),
                *truth,
            )
        })
        .collect();
    let input = NarrativeInput {
        turn: TurnNumber::new(1),
        semantic_results: &semantic_results,
    };
    graph().resolve(state, &input)
}

fn resolve(state: &NarrativeState, results: &[(&str, Truth)]) -> Vec<(String, NodeStatus)> {
    resolution(state, results)
        .transitions
        .into_iter()
        .map(|transition| (transition.node_id.to_string(), transition.to))
        .collect()
}

#[test]
fn story_start_activates_only_nodes_whose_condition_holds() {
    let transitions = resolve(&NarrativeState::default(), &[]);

    assert_eq!(transitions, [("arrival".to_owned(), NodeStatus::Active)]);
}

#[test]
fn active_node_completes_when_semantic_condition_is_satisfied() {
    let transitions = resolve(&state(&[("arrival", NodeStatus::Active)]), &[("identified", Truth::Satisfied)]);

    assert_eq!(transitions, [("arrival".to_owned(), NodeStatus::Completed)]);
}

#[test]
fn completion_does_not_activate_successor_in_the_same_turn() {
    let transitions = resolve(&state(&[("arrival", NodeStatus::Active)]), &[("identified", Truth::Satisfied)]);

    assert!(transitions.iter().all(|(id, _)| id != "reveal"));
}

#[test]
fn successor_activates_on_the_turn_after_completion() {
    let transitions = resolve(&state(&[("arrival", NodeStatus::Completed)]), &[]);

    assert_eq!(transitions, [("reveal".to_owned(), NodeStatus::Active)]);
}

#[test]
fn unknown_or_unsatisfied_semantic_result_does_not_transition() {
    let active = state(&[("arrival", NodeStatus::Active)]);

    assert!(resolve(&active, &[]).is_empty());
    assert!(resolve(&active, &[("identified", Truth::Unknown)]).is_empty());
    assert!(resolve(&active, &[("identified", Truth::Unsatisfied)]).is_empty());
}

#[test]
fn skip_takes_precedence_over_complete() {
    let transitions = resolve(
        &state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Active)]),
        &[("left", Truth::Satisfied), ("revealed", Truth::Satisfied)],
    );

    assert_eq!(transitions, [("reveal".to_owned(), NodeStatus::Skipped)]);
}

#[test]
fn finished_nodes_never_transition_again() {
    let finished = state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Skipped)]);

    assert!(resolve(&finished, &[("left", Truth::Satisfied), ("revealed", Truth::Satisfied)]).is_empty());
}

#[test]
fn completing_terminal_node_completes_story() {
    let resolution = resolution(
        &state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Active)]),
        &[("revealed", Truth::Satisfied)],
    );

    assert!(resolution.story_completed);
}

#[test]
fn skipping_terminal_node_does_not_complete_story() {
    let resolution = resolution(
        &state(&[("arrival", NodeStatus::Completed), ("reveal", NodeStatus::Active)]),
        &[("left", Truth::Satisfied)],
    );

    assert!(!resolution.story_completed);
}

#[test]
fn completing_non_terminal_node_does_not_complete_story() {
    let resolution = resolution(&state(&[("arrival", NodeStatus::Active)]), &[("identified", Truth::Satisfied)]);

    assert!(!resolution.story_completed);
}

#[test]
fn applying_resolution_advances_the_graph_one_step_per_turn() {
    let mut narrative_state = NarrativeState::default();
    let graph = graph();
    let semantic_results = BTreeMap::new();
    let input = NarrativeInput {
        turn: TurnNumber::new(1),
        semantic_results: &semantic_results,
    };

    narrative_state.apply(&graph.resolve(&narrative_state, &input).transitions);
    assert_eq!(
        narrative_state.status(&NarrativeNodeId::try_new("arrival").expect("test node id should be valid")),
        NodeStatus::Active
    );
    assert!(graph.resolve(&narrative_state, &input).transitions.is_empty());
}
