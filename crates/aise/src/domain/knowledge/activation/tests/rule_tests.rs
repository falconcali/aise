use super::*;

#[test]
fn disabled_rule_accepts_positive_limits() {
    let limits = ActivationRuleLimits {
        max_primary_patterns_per_entry: 1,
        max_secondary_patterns_per_entry: 1,
        max_pattern_bytes: 1,
        max_regex_program_bytes: 1,
        max_groups_per_entry: 1,
        max_group_key_bytes: 1,
        max_scan_depth: 1,
    };
    assert!(KnowledgeActivationRule::disabled().validate(limits).is_ok());
}
