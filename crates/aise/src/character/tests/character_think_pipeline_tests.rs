use super::*;
use crate::config::CharacterThinkConfig;

#[test]
fn character_decision_dto_parses_required_decision_with_empty_utterance() {
    let dto: CharacterDecisionDto =
        serde_json::from_str(r#"{"decision":"wait quietly","suggested_utterance":""}"#).expect("dto");
    assert_eq!(dto.decision, "wait quietly");
    assert_eq!(dto.suggested_utterance, "");
}

#[test]
fn character_decision_dto_parses_present_utterance() {
    let dto: CharacterDecisionDto =
        serde_json::from_str(r#"{"decision":"wait","suggested_utterance":"let's go"}"#).expect("dto");
    assert_eq!(dto.suggested_utterance, "let's go");
}

#[test]
fn character_decision_dto_rejects_unknown_field() {
    let parsed: Result<CharacterDecisionDto, _> =
        serde_json::from_str(r#"{"decision":"wait","suggested_utterance":"","extra":1}"#);
    assert!(parsed.is_err());
}

#[test]
fn character_decision_dto_rejects_each_removed_field() {
    for field in ["perception", "emotion", "goal", "possible_action"] {
        let payload = format!(r#"{{"decision":"wait","suggested_utterance":"","{field}":"x"}}"#);
        let parsed: Result<CharacterDecisionDto, _> = serde_json::from_str(&payload);
        assert!(parsed.is_err(), "field {field} should be rejected");
    }
}

#[test]
fn character_decision_dto_rejects_model_returned_character_id() {
    let parsed: Result<CharacterDecisionDto, _> =
        serde_json::from_str(r#"{"decision":"wait","suggested_utterance":"","character_id":"npc-1"}"#);
    assert!(parsed.is_err());
}

#[test]
fn character_decision_dto_rejects_missing_or_null_decision() {
    let missing: Result<CharacterDecisionDto, _> = serde_json::from_str(r#"{"suggested_utterance":""}"#);
    assert!(missing.is_err());
    let null: Result<CharacterDecisionDto, _> = serde_json::from_str(r#"{"decision":null,"suggested_utterance":""}"#);
    assert!(null.is_err());
}

#[test]
fn character_decision_contract_schema_omits_schema_uri_and_disallows_extra_fields() {
    let config = CharacterThinkConfig {
        max_field_bytes: 512,
        ..CharacterThinkConfig::default()
    };
    let contract = character_decision_contract(&config);
    assert!(contract.schema.get("$schema").is_none());
    assert_eq!(contract.schema["additionalProperties"], false);
}

#[test]
fn character_decision_contract_validate_rejects_whitespace_only_decision() {
    let config = CharacterThinkConfig::default();
    let contract = character_decision_contract(&config);
    let dto = CharacterDecisionDto {
        decision: "   ".to_owned(),
        suggested_utterance: String::new(),
    };
    assert!((contract.validate)(&dto).is_err());
}

#[test]
fn character_decision_contract_validate_accepts_non_empty_decision() {
    let config = CharacterThinkConfig::default();
    let contract = character_decision_contract(&config);
    let dto = CharacterDecisionDto {
        decision: "wait".to_owned(),
        suggested_utterance: String::new(),
    };
    assert!((contract.validate)(&dto).is_ok());
}

#[test]
fn normalize_required_output_rejects_whitespace_only_decision() {
    let result = normalize_required_output("   ".to_owned(), "decision", 32);
    assert!(result.is_err());
}

#[test]
fn normalize_optional_output_maps_empty_string_to_none() {
    let result = normalize_optional_output(String::new(), "suggested_utterance", 32).unwrap();
    assert!(result.is_none());
}

#[test]
fn normalize_optional_output_maps_whitespace_only_to_none() {
    let result = normalize_optional_output("   ".to_owned(), "suggested_utterance", 32).unwrap();
    assert!(result.is_none());
}

#[test]
fn normalize_required_output_trims_surrounding_whitespace() {
    let result = normalize_required_output("  wait  ".to_owned(), "decision", 32).expect("normalized");
    assert_eq!(result.as_str(), "wait");
}

#[test]
fn normalize_required_output_enforces_per_field_byte_limit() {
    let long = "x".repeat(64);
    let result = normalize_required_output(long, "decision", 8);
    assert!(result.is_err());
}

#[test]
fn normalize_optional_output_enforces_per_field_byte_limit() {
    let long = "x".repeat(64);
    let result = normalize_optional_output(long, "suggested_utterance", 8);
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
