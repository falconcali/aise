use super::*;
use crate::domain::ids::CharacterId;

#[test]
fn parse_thought_reads_all_fields() {
    let thought = parse_thought(
        r#"{"perception":"loud crowd","emotion":"anxious","goal":"find Tom","possible_action":"push through"}"#,
        CharacterId::from("c-1"),
    )
    .expect("valid thought");
    assert_eq!(thought.character_id.as_str(), "c-1");
    assert_eq!(thought.perception, "loud crowd");
    assert_eq!(thought.emotion, "anxious");
    assert_eq!(thought.goal, "find Tom");
    assert_eq!(thought.possible_action, "push through");
}

#[test]
fn parse_thought_fills_missing_fields_with_defaults() {
    let thought = parse_thought(r#"{"perception":"only this"}"#, CharacterId::from("c-2")).expect("partial thought");
    assert_eq!(thought.perception, "only this");
    assert_eq!(thought.emotion, "");
    assert_eq!(thought.goal, "");
    assert_eq!(thought.possible_action, "");
}

#[test]
fn bound_field_caps_characters() {
    let long = "x".repeat(1000);
    let bounded = bound_field(&long);
    assert_eq!(bounded.chars().count(), super::MAX_THOUGHT_FIELD_CHARS);
}

#[test]
fn parse_thought_rejects_invalid_json() {
    assert!(parse_thought("nope", CharacterId::from("c-1")).is_err());
}
