use super::*;
use crate::domain::turn::character::CharacterThoughtOutput;

#[test]
fn character_thought_output_parses_all_fields() {
    let output: CharacterThoughtOutput =
        serde_json::from_str(r#"{"perception":"saw rain","emotion":"calm","goal":"wait","possible_action":"shelter"}"#)
            .expect("thought");
    assert_eq!(output.perception.as_str(), "saw rain");
    assert_eq!(output.emotion.as_str(), "calm");
    assert_eq!(output.goal.as_str(), "wait");
    assert_eq!(output.possible_action.as_str(), "shelter");
}

#[test]
fn character_thought_output_rejects_unknown_fields() {
    let parsed: Result<CharacterThoughtOutput, _> =
        serde_json::from_str(r#"{"perception":"a","emotion":"b","goal":"c","possible_action":"d","extra":1}"#);
    assert!(parsed.is_err());
}

#[test]
fn bounded_text_rejects_oversized_thought_field() {
    let long = "x".repeat(2048);
    let result = BoundedText::try_new(long, "perception", 1024);
    assert!(result.is_err());
}

#[test]
fn character_thought_output_rejects_empty_normalized_field() {
    let result = normalize_output(
        BoundedText::try_new("   ", "perception", 16).expect("bounded"),
        "perception",
        16,
    );
    assert!(result.is_err());
}

#[test]
fn character_thought_output_normalizes_surrounding_whitespace() {
    let result = normalize_output(
        BoundedText::try_new("  cautious  ", "perception", 32).expect("bounded"),
        "perception",
        32,
    )
    .expect("normalized");
    assert_eq!(result.as_str(), "cautious");
}
