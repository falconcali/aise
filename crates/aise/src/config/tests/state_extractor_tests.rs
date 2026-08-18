use super::*;

#[test]
fn default_config_is_valid() {
    StateExtractorConfig::default()
        .validate()
        .expect("default config must validate");
}

#[test]
fn rejects_zero_new_roles_per_turn() {
    let config = StateExtractorConfig {
        max_new_roles_per_turn: 0,
        ..StateExtractorConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn rejects_new_roles_per_turn_above_hard_bound() {
    let config = StateExtractorConfig {
        max_new_roles_per_turn: MAX_NEW_ROLES_PER_TURN_HARD_BOUND + 1,
        ..StateExtractorConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn accepts_new_roles_per_turn_at_hard_bound() {
    let config = StateExtractorConfig {
        max_new_roles_per_turn: MAX_NEW_ROLES_PER_TURN_HARD_BOUND,
        ..StateExtractorConfig::default()
    };
    config.validate().expect("hard bound is inclusive");
}

#[test]
fn rejects_zero_role_profile_bytes() {
    let config = StateExtractorConfig {
        max_role_profile_bytes: 0,
        ..StateExtractorConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn rejects_zero_cast_policy_violations() {
    let config = StateExtractorConfig {
        max_cast_policy_violations: 0,
        ..StateExtractorConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn rejects_zero_knowledge_items() {
    let config = StateExtractorConfig {
        max_knowledge_items: 0,
        ..StateExtractorConfig::default()
    };
    assert!(config.validate().is_err());
}
