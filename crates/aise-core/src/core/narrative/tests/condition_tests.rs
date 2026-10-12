use super::*;
use crate::core::NarrativeTransition;
use serde_json::json;

fn condition(value: serde_json::Value) -> NarrativeCondition {
    serde_json::from_value(value).expect("test condition should deserialize")
}

fn evaluate(condition: &NarrativeCondition, turn: u64, results: &[(&str, Truth)]) -> Truth {
    evaluate_with_state(condition, &NarrativeState::default(), turn, results)
}

fn evaluate_with_state(
    condition: &NarrativeCondition,
    state: &NarrativeState,
    turn: u64,
    results: &[(&str, Truth)],
) -> Truth {
    let semantic_results = results
        .iter()
        .map(|(id, truth)| {
            (
                NarrativeConditionId::try_new(*id).expect("test condition id should be valid"),
                *truth,
            )
        })
        .collect();
    let input = NarrativeInput {
        turn: TurnNumber::new(turn),
        semantic_results: &semantic_results,
    };
    condition.evaluate(state, &input)
}

fn semantic(id: &str) -> serde_json::Value {
    json!({ "type": "semantic", "condition_id": id, "criterion": format!("criterion of {id}") })
}

#[test]
fn all_is_unsatisfied_when_any_child_is_unsatisfied_even_with_unknown() {
    let all = condition(json!({ "type": "all", "conditions": [semantic("a"), semantic("b")] }));

    assert_eq!(evaluate(&all, 0, &[("a", Truth::Unsatisfied)]), Truth::Unsatisfied);
}

#[test]
fn all_is_unknown_when_remaining_children_are_satisfied_or_unknown() {
    let all = condition(json!({ "type": "all", "conditions": [semantic("a"), semantic("b")] }));

    assert_eq!(evaluate(&all, 0, &[("a", Truth::Satisfied)]), Truth::Unknown);
}

#[test]
fn any_is_satisfied_when_any_child_is_satisfied_even_with_unknown() {
    let any = condition(json!({ "type": "any", "conditions": [semantic("a"), semantic("b")] }));

    assert_eq!(evaluate(&any, 0, &[("b", Truth::Satisfied)]), Truth::Satisfied);
}

#[test]
fn any_is_unknown_when_no_child_is_satisfied_and_some_are_unknown() {
    let any = condition(json!({ "type": "any", "conditions": [semantic("a"), semantic("b")] }));

    assert_eq!(evaluate(&any, 0, &[("a", Truth::Unsatisfied)]), Truth::Unknown);
}

#[test]
fn not_keeps_unknown_and_flips_known_values() {
    let not = condition(json!({ "type": "not", "condition": semantic("a") }));

    assert_eq!(evaluate(&not, 0, &[]), Truth::Unknown);
    assert_eq!(evaluate(&not, 0, &[("a", Truth::Satisfied)]), Truth::Unsatisfied);
    assert_eq!(evaluate(&not, 0, &[("a", Truth::Unsatisfied)]), Truth::Satisfied);
}

#[test]
fn turn_reaches_is_inclusive() {
    let reaches = condition(json!({ "type": "turn_reaches", "turn": 3 }));

    assert_eq!(evaluate(&reaches, 2, &[]), Truth::Unsatisfied);
    assert_eq!(evaluate(&reaches, 3, &[]), Truth::Satisfied);
}

#[test]
fn node_is_reads_current_state() {
    let node_is = condition(json!({ "type": "node_is", "node_id": "arrival", "status": "completed" }));
    let mut state = NarrativeState::default();

    assert_eq!(evaluate_with_state(&node_is, &state, 0, &[]), Truth::Unsatisfied);

    state.apply(&[NarrativeTransition {
        node_id: NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        to: NodeStatus::Completed,
    }]);
    assert_eq!(evaluate_with_state(&node_is, &state, 0, &[]), Truth::Satisfied);
}

#[test]
fn story_started_is_always_satisfied() {
    assert_eq!(
        evaluate(&condition(json!({ "type": "story_started" })), 0, &[]),
        Truth::Satisfied
    );
}

#[test]
fn unknown_condition_type_is_rejected() {
    let result = serde_json::from_value::<NarrativeCondition>(json!({ "type": "event_occurred" }));

    assert!(result.is_err());
}
