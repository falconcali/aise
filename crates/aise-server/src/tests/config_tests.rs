use super::*;

#[test]
fn api_key_resolves_named_environment_variable() {
    let get_env = |name: &str| match name {
        "DEEPSEEK_API_KEY" => Some("sk-real".into()),
        _ => None,
    };
    assert_eq!(resolve_api_key("DEEPSEEK_API_KEY".into(), get_env), Some("sk-real".into()));
}

#[test]
fn api_key_resolves_env_prefix() {
    let get_env = |name: &str| match name {
        "DEEPSEEK_API_KEY" => Some("sk-real".into()),
        _ => None,
    };
    assert_eq!(resolve_api_key("env:DEEPSEEK_API_KEY".into(), get_env), Some("sk-real".into()));
}

#[test]
fn api_key_env_prefix_missing_returns_none() {
    let get_env = |_: &str| None;
    assert_eq!(resolve_api_key("env:MISSING_KEY".into(), get_env), None);
}

#[test]
fn api_key_falls_back_to_literal_when_environment_unset() {
    let get_env = |_: &str| None;
    assert_eq!(resolve_api_key("sk-literal".into(), get_env), Some("sk-literal".into()));
}

#[test]
fn api_key_ignores_empty_environment_value() {
    let get_env = |_: &str| Some(String::new());
    assert_eq!(
        resolve_api_key("DEEPSEEK_API_KEY".into(), get_env),
        Some("DEEPSEEK_API_KEY".into())
    );
}

#[test]
fn thinking_env_override_parses_modes() {
    use aise::config::ThinkingMode;
    let mut config = ServerConfig::default();
    assert_eq!(config.aise.llm.thinking, None);
    let get_env = |name: &str| match name {
        "AISE_LLM_THINKING" => Some("disabled".to_string()),
        _ => None,
    };
    config.apply_env_overrides_with(get_env).unwrap();
    assert_eq!(config.aise.llm.thinking, Some(ThinkingMode::Disabled));
}

#[test]
fn invalid_toml_fails_load() {
    let path = PathBuf::from("config/aise_config.toml");
    let result =
        toml::from_str::<ServerConfig>("listen_addr = \"not-an-address\"\nmax_sessions = \n").map_err(|source| {
            ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            }
        });
    assert!(result.is_err(), "invalid TOML must fail the ServerConfig::load parse step");
}

#[test]
fn invalid_env_override_fails_load() {
    let mut config = ServerConfig::default();
    let get_env = |name: &str| match name {
        "AISE_LISTEN_ADDR" => Some("not-an-address".to_string()),
        _ => None,
    };
    let result = config.apply_env_overrides_with(get_env);
    assert!(
        result.is_err(),
        "invalid environment override must fail the ServerConfig::load env step"
    );
}

#[test]
fn env_override_applies_on_load() {
    let mut config = ServerConfig::default();
    let get_env = |name: &str| match name {
        "AISE_MAX_SESSIONS" => Some("12".to_string()),
        _ => None,
    };
    config.apply_env_overrides_with(get_env).unwrap();
    assert_eq!(config.max_sessions, 12);
}

#[test]
fn zero_max_sessions_fails_validate() {
    let config = ServerConfig {
        max_sessions: 0,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_err(), "zero capacities must fail validate");
}
