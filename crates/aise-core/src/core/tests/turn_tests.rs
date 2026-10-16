use super::*;

#[test]
fn turn_status_serializes_only_pass_and_choice() {
    assert_eq!(serde_json::to_string(&TurnStatus::Pass).expect("pass serializes"), "\"Pass\"");
    assert_eq!(
        serde_json::to_string(&TurnStatus::Choice).expect("choice serializes"),
        "\"Choice\""
    );
}

#[test]
fn legacy_turn_status_values_are_rejected() {
    assert!(serde_json::from_str::<TurnStatus>("\"Accepted\"").is_err());
    assert!(serde_json::from_str::<TurnStatus>("\"Rejected\"").is_err());
}

#[test]
fn empty_turn_evaluation_is_rejected() {
    assert!(serde_json::from_str::<TurnEvaluation>("{}").is_err());
}
