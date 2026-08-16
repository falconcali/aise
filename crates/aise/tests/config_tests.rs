use aise::AiseConfig;
use aise::config::{ContextPreparationConfig, ThinkingMode};

#[test]
fn turn_max_tokens_alias_maps_to_max_output_tokens() {
    let config: AiseConfig = serde_json::from_str(r#"{"turn":{"max_tokens":4096}}"#).expect("parse config");
    assert_eq!(config.turn.max_output_tokens, 4096);
}

#[test]
fn llm_thinking_parses_from_config() {
    let config: AiseConfig = serde_json::from_str(r#"{"llm":{"thinking":"disabled"}}"#).expect("parse config");
    assert_eq!(config.llm.thinking, Some(ThinkingMode::Disabled));
    let default = AiseConfig::default();
    assert_eq!(default.llm.thinking, None, "unset thinking must not send a toggle");
}

#[test]
fn default_turn_budget_leaves_room_for_thinking() {
    let config = AiseConfig::default();
    assert_eq!(config.turn.max_output_tokens, 4096);
    assert!(
        config.turn.max_total_tokens >= config.turn.max_input_tokens + config.turn.max_output_tokens,
        "total budget must cover input plus output"
    );
}

#[test]
fn context_dialogue_example_limits_have_bounded_defaults() {
    let config = ContextPreparationConfig::default();
    assert_eq!(config.max_dialogue_examples_per_role, 4);
    assert_eq!(config.max_dialogue_example_tokens_per_role, 256);
    assert!(config.validate().is_ok());
}

#[test]
fn context_dialogue_example_limits_must_be_positive() {
    let config = ContextPreparationConfig {
        max_dialogue_examples_per_role: 0,
        ..ContextPreparationConfig::default()
    };
    assert!(config.validate().is_err());
    let config = ContextPreparationConfig {
        max_dialogue_example_tokens_per_role: 0,
        ..ContextPreparationConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn context_dialogue_example_legacy_keys_are_rejected() {
    let mut value = serde_json::to_value(ContextPreparationConfig::default()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("max_dialogue_examples_per_character".into(), serde_json::json!(4));
    assert!(serde_json::from_value::<ContextPreparationConfig>(value).is_err());
}
