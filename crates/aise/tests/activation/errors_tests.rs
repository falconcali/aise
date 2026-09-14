use aise::config::ActivationConfig;
use aise::context::ContextError;
use aise::domain::knowledge::activation::{
    ActivationError, ActivationPattern, ActivationRuleLimits, ActivationRuleValidationError, ActivationStoreFailure,
    KnowledgeActivationRule,
};

fn rule_limits() -> ActivationRuleLimits {
    ActivationConfig::default().rule.limits()
}

fn enabled_rule(key: &str) -> KnowledgeActivationRule {
    let mut rule = KnowledgeActivationRule::disabled();
    rule.mode.enabled = true;
    rule.match_rule.keys = vec![ActivationPattern::Literal(key.to_owned())];
    rule
}

#[test]
fn every_activation_error_maps_to_a_stable_turn_code() {
    let cases = [
        (ActivationError::InvalidRule { code: "any" }, "activation_invalid_rule"),
        (ActivationError::InvalidRegex, "activation_invalid_regex"),
        (ActivationError::IndexVersionMismatch, "activation_index_version_mismatch"),
        (ActivationError::SnapshotMismatch, "activation_snapshot_mismatch"),
        (ActivationError::ContinuationMismatch, "activation_continuation_mismatch"),
        (
            ActivationError::WorkLimitExceeded { limit: "any" },
            "activation_work_limit_exceeded",
        ),
        (ActivationError::RecursionLimitReached, "activation_recursion_limit_reached"),
        (ActivationError::MandatoryBudgetExceeded, "activation_mandatory_budget_exceeded"),
        (
            ActivationError::ExternalTargetUnauthorized,
            "activation_external_target_unauthorized",
        ),
        (ActivationError::TimedStateInconsistent, "activation_timed_state_inconsistent"),
        (
            ActivationError::ProviderFailure { provider: "any" },
            "activation_provider_failure",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.code(), expected);
        assert_eq!(ContextError::Activation(error).turn_code(), expected);
    }
}

#[test]
fn store_failures_reuse_the_existing_store_codes() {
    assert_eq!(
        ContextError::Activation(ActivationError::Store(ActivationStoreFailure::RevisionConflict)).turn_code(),
        "retrieval_snapshot_conflict"
    );
    assert_eq!(
        ContextError::Activation(ActivationError::Store(ActivationStoreFailure::Unavailable)).turn_code(),
        "store_error"
    );
}

#[test]
fn rule_limits_are_enforced_field_by_field() {
    let mut rule = enabled_rule("alpha");
    rule.match_rule.keys = (0..64).map(|index| ActivationPattern::Literal(format!("key{index}"))).collect();
    let mut limits = rule_limits();
    limits.max_primary_patterns_per_entry = 4;
    assert_eq!(
        rule.validate(limits).unwrap_err(),
        ActivationRuleValidationError::TooManyPrimaryPatterns
    );

    let mut rule = enabled_rule("alpha");
    rule.match_rule.secondary_keys = (0..64).map(|index| ActivationPattern::Literal(format!("key{index}"))).collect();
    let mut limits = rule_limits();
    limits.max_secondary_patterns_per_entry = 4;
    assert_eq!(
        rule.validate(limits).unwrap_err(),
        ActivationRuleValidationError::TooManySecondaryPatterns
    );

    let rule = enabled_rule("a-very-long-activation-pattern");
    let mut limits = rule_limits();
    limits.max_pattern_bytes = 4;
    assert_eq!(
        rule.validate(limits).unwrap_err(),
        ActivationRuleValidationError::PatternTooLong
    );

    let rule = enabled_rule("/(a|b|c){10,20}/i");
    let mut limits = rule_limits();
    limits.max_regex_program_bytes = 8;
    assert_eq!(
        rule.validate(limits).unwrap_err(),
        ActivationRuleValidationError::RegexProgramTooLarge
    );

    let rule = enabled_rule("/(/i");
    assert_eq!(
        rule.validate(rule_limits()).unwrap_err(),
        ActivationRuleValidationError::InvalidRegex
    );

    let mut limits = rule_limits();
    limits.max_pattern_bytes = 0;
    assert_eq!(
        enabled_rule("alpha").validate(limits).unwrap_err(),
        ActivationRuleValidationError::InvalidLimit
    );
}

#[test]
fn valid_rules_pass_the_default_limits() {
    assert!(enabled_rule("alpha").validate(rule_limits()).is_ok());
    assert!(enabled_rule("/alpha/i").validate(rule_limits()).is_ok());
    assert!(KnowledgeActivationRule::disabled().validate(rule_limits()).is_ok());
}
