use super::*;
use crate::core::RoleId;

fn role_id() -> RoleId {
    RoleId::try_new("target").expect("valid role id")
}

fn error_code(raw: &str, config: &ThinkConfig) -> &'static str {
    match parse_character_decision(raw, &role_id(), config).expect_err("output should fail") {
        PipelineError::InvalidOutput { code, .. } => code,
        error => panic!("unexpected error: {error}"),
    }
}

#[test]
fn valid_output_trims_fields_and_binds_engine_role_id() {
    let output = parse_character_decision(
        r#"{"decision":"  wait and watch  ","suggested_utterance":"  Who goes there?  "}"#,
        &role_id(),
        &ThinkConfig::default(),
    )
    .expect("output should parse");

    assert_eq!(output.role_id.as_str(), "target");
    assert_eq!(output.decision.as_str(), "wait and watch");
    assert_eq!(
        output.suggested_utterance.as_ref().map(|value| value.as_str()),
        Some("Who goes there?")
    );
}

#[test]
fn missing_and_null_utterance_normalize_to_none() {
    let missing = parse_character_decision(r#"{"decision":"wait"}"#, &role_id(), &ThinkConfig::default())
        .expect("missing utterance should parse");
    let null = parse_character_decision(
        r#"{"decision":"wait","suggested_utterance":null}"#,
        &role_id(),
        &ThinkConfig::default(),
    )
    .expect("null utterance should parse");

    assert!(missing.suggested_utterance.is_none());
    assert!(null.suggested_utterance.is_none());
}

#[test]
fn malformed_or_unknown_fields_use_invalid_json_code() {
    assert_eq!(error_code("not json", &ThinkConfig::default()), "think_output_invalid_json");
    assert_eq!(
        error_code(r#"{"decision":"wait","role_id":"model-choice"}"#, &ThinkConfig::default()),
        "think_output_invalid_json"
    );
}

#[test]
fn empty_decision_uses_stable_code() {
    assert_eq!(
        error_code(r#"{"decision":"   "}"#, &ThinkConfig::default()),
        "think_decision_empty"
    );
}

#[test]
fn oversized_decision_uses_stable_code() {
    let config = ThinkConfig {
        max_decision_bytes: 3,
        ..ThinkConfig::default()
    };

    assert_eq!(error_code(r#"{"decision":"four"}"#, &config), "think_decision_too_long");
}

#[test]
fn empty_suggested_utterance_uses_stable_code() {
    assert_eq!(
        error_code(r#"{"decision":"wait","suggested_utterance":"  "}"#, &ThinkConfig::default()),
        "think_suggested_utterance_empty"
    );
}

#[test]
fn oversized_suggested_utterance_uses_stable_code() {
    let config = ThinkConfig {
        max_suggested_utterance_bytes: 3,
        ..ThinkConfig::default()
    };

    assert_eq!(
        error_code(r#"{"decision":"wait","suggested_utterance":"four"}"#, &config),
        "think_suggested_utterance_too_long"
    );
}

#[test]
fn oversized_combined_output_uses_stable_code() {
    let config = ThinkConfig {
        max_total_output_bytes: 7,
        ..ThinkConfig::default()
    };

    assert_eq!(
        error_code(r#"{"decision":"four","suggested_utterance":"four"}"#, &config),
        "think_output_too_long"
    );
}
