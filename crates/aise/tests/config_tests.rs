use aise::AiseConfig;
use aise::config::ThinkingMode;

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
