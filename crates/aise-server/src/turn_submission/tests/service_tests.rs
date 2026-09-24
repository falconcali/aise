use super::*;

#[test]
fn idempotency_digest_is_stable_and_does_not_expose_key() {
    let value = digest("secret-idempotency-key");

    assert_eq!(value.len(), 64);
    assert_eq!(value, "9ab3afd1220a87823137dd9a1803c652c5361facc5f279c6254889a295720341");
    assert!(!value.contains("secret-idempotency-key"));
}

#[test]
fn submission_errors_have_stable_codes() {
    assert_eq!(submission_error_code(&TurnSubmissionError::InvalidSession), "invalid_session");
    assert_eq!(
        submission_error_code(&TurnSubmissionError::SessionNotFound),
        "session_not_found"
    );
    assert_eq!(
        submission_error_code(&TurnSubmissionError::MissingIdempotencyKey),
        "missing_idempotency_key"
    );
}
