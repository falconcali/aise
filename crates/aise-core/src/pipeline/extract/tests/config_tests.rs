use super::*;

#[test]
fn default_config_has_bounded_nonzero_limits() {
    let config = ExtractConfig::default();

    assert_eq!(config.max_fact_changes, 8);
    assert_eq!(config.max_rumor_changes, 8);
    assert_eq!(config.max_roles_for_memory, 4);
    assert_eq!(config.max_memory_changes_per_role, 4);
    assert_eq!(config.max_memory_concurrency.get(), 4);
    config.validate().expect("default config should be valid");
}

#[test]
fn config_rejects_unknown_fields() {
    let result = serde_json::from_value::<ExtractConfig>(serde_json::json!({
        "max_fact_changes": 8,
        "max_rumor_changes": 8,
        "max_roles_for_memory": 4,
        "max_memory_changes_per_role": 4,
        "max_memory_concurrency": 4,
        "unexpected": true
    }));

    assert!(result.is_err());
}
