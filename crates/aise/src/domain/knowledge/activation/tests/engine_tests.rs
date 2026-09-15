use super::*;

#[test]
fn stable_error_codes_cover_snapshot_and_budget_failures() {
    assert_eq!(ActivationError::SnapshotMismatch.code(), "activation_snapshot_conflict");
    assert_eq!(ActivationError::MandatoryBudgetExceeded.code(), "activation_mandatory_budget");
}
