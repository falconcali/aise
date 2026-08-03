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
