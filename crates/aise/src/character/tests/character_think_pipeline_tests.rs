use super::*;

#[test]
fn character_decision_output_parses_required_decision_with_omitted_suggestion() {
    let output: CharacterDecisionOutput = serde_json::from_str(r#"{"decision":"wait quietly"}"#).expect("output");
    assert_eq!(output.decision.as_str(), "wait quietly");
    assert!(output.suggested_utterance.is_none());
}

#[test]
fn character_decision_output_parses_present_utterance_as_some() {
    let output: CharacterDecisionOutput =
        serde_json::from_str(r#"{"decision":"wait","suggested_utterance":"let's go"}"#).expect("output");
    assert_eq!(
        output.suggested_utterance.map(|value| value.as_str().to_owned()),
        Some("let's go".to_owned())
    );
}

#[test]
fn character_decision_output_parses_explicit_null_utterance_as_none() {
    let output: CharacterDecisionOutput =
        serde_json::from_str(r#"{"decision":"wait","suggested_utterance":null}"#).expect("output");
    assert!(output.suggested_utterance.is_none());
}

#[test]
fn character_decision_output_rejects_unknown_field() {
    let parsed: Result<CharacterDecisionOutput, _> = serde_json::from_str(r#"{"decision":"wait","extra":1}"#);
    assert!(parsed.is_err());
}

#[test]
fn character_decision_output_rejects_each_removed_field() {
    for field in ["perception", "emotion", "goal", "possible_action"] {
        let payload = format!(r#"{{"decision":"wait","{field}":"x"}}"#);
        let parsed: Result<CharacterDecisionOutput, _> = serde_json::from_str(&payload);
        assert!(parsed.is_err(), "field {field} should be rejected");
    }
}

#[test]
fn character_decision_output_rejects_model_returned_character_id() {
    let parsed: Result<CharacterDecisionOutput, _> =
        serde_json::from_str(r#"{"decision":"wait","character_id":"npc-1"}"#);
    assert!(parsed.is_err());
}

#[test]
fn character_decision_output_rejects_missing_or_null_decision() {
    let missing: Result<CharacterDecisionOutput, _> = serde_json::from_str(r#"{}"#);
    assert!(missing.is_err());
    let null: Result<CharacterDecisionOutput, _> = serde_json::from_str(r#"{"decision":null}"#);
    assert!(null.is_err());
}

#[test]
fn normalize_required_output_rejects_whitespace_only_decision() {
    let result = normalize_required_output(BoundedText::try_new("   ", "decision", 32).unwrap(), "decision", 32);
    assert!(result.is_err());
}

#[test]
fn normalize_optional_output_rejects_whitespace_only_present_utterance() {
    let result = normalize_optional_output(
        Some(BoundedText::try_new("   ", "suggested_utterance", 32).unwrap()),
        "suggested_utterance",
        32,
    );
    assert!(result.is_err());
}

#[test]
fn normalize_optional_output_maps_absent_to_none() {
    let result = normalize_optional_output(None, "suggested_utterance", 32).unwrap();
    assert!(result.is_none());
}

#[test]
fn normalize_required_output_trims_surrounding_whitespace() {
    let result = normalize_required_output(BoundedText::try_new("  wait  ", "decision", 32).unwrap(), "decision", 32)
        .expect("normalized");
    assert_eq!(result.as_str(), "wait");
}

#[test]
fn normalize_required_output_enforces_per_field_byte_limit() {
    let long = "x".repeat(64);
    let result = normalize_required_output(BoundedText::try_new(long, "decision", 4096).unwrap(), "decision", 8);
    assert!(result.is_err());
}

#[test]
fn normalize_optional_output_enforces_per_field_byte_limit() {
    let long = "x".repeat(64);
    let result = normalize_optional_output(
        Some(BoundedText::try_new(long, "suggested_utterance", 4096).unwrap()),
        "suggested_utterance",
        8,
    );
    assert!(result.is_err());
}

#[test]
fn enforce_total_output_budget_accepts_combined_bytes_within_limit() {
    let decision = BoundedText::try_new("wait", "decision", 32).unwrap();
    let utterance = BoundedText::try_new("ok", "suggested_utterance", 32).unwrap();
    let total = enforce_total_output_budget(&decision, Some(&utterance), 16).expect("within budget");
    assert_eq!(total, "wait".len() + "ok".len());
}

#[test]
fn enforce_total_output_budget_rejects_combined_bytes_over_limit() {
    let decision = BoundedText::try_new("a decision long enough", "decision", 64).unwrap();
    let utterance = BoundedText::try_new("an utterance also long enough", "suggested_utterance", 64).unwrap();
    let error = enforce_total_output_budget(&decision, Some(&utterance), 8).unwrap_err();
    assert_eq!(error.code(), "model_output_invalid");
}
