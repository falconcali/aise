use super::*;

#[test]
fn rejection_counts_project_every_reason() {
    let summary = [
        (ActivationRejectionReason::Disabled, 1),
        (ActivationRejectionReason::ScopeMismatch, 2),
        (ActivationRejectionReason::Delayed, 3),
        (ActivationRejectionReason::Cooldown, 4),
        (ActivationRejectionReason::RecursionExcluded, 5),
        (ActivationRejectionReason::RecursionLevelLocked, 6),
        (ActivationRejectionReason::SecondaryCondition, 7),
        (ActivationRejectionReason::GroupLoser, 8),
        (ActivationRejectionReason::Probability, 9),
        (ActivationRejectionReason::Budget, 10),
        (ActivationRejectionReason::Duplicate, 11),
        (ActivationRejectionReason::WorkLimit, 12),
    ]
    .into_iter()
    .collect();
    let counts = ActivationRejectionCounts::from_summary(&summary);
    assert_eq!(counts.disabled, 1);
    assert_eq!(counts.scope_mismatch, 2);
    assert_eq!(counts.delayed, 3);
    assert_eq!(counts.cooldown, 4);
    assert_eq!(counts.recursion_excluded, 5);
    assert_eq!(counts.recursion_level_locked, 6);
    assert_eq!(counts.secondary_condition, 7);
    assert_eq!(counts.group_loser, 8);
    assert_eq!(counts.probability, 9);
    assert_eq!(counts.budget, 10);
    assert_eq!(counts.duplicate, 11);
    assert_eq!(counts.work_limit, 12);
}

#[test]
fn bounded_digest_is_stable_at_zero_budget() {
    assert_eq!(bounded_evidence_digest(&[], 0), bounded_evidence_digest(&[], 0));
}
