use super::*;

#[test]
fn first_turn_non_empty_summary_requires_repair() {
    assert!(summary_requires_pre_turn_sequence(Some("summary"), false));
}

#[test]
fn first_turn_null_or_empty_summary_is_valid() {
    assert!(!summary_requires_pre_turn_sequence(None, false));
    assert!(!summary_requires_pre_turn_sequence(Some("  "), false));
}

#[test]
fn later_turn_summary_is_valid() {
    assert!(!summary_requires_pre_turn_sequence(Some("summary"), true));
}
