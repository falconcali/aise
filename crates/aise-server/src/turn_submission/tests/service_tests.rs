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

#[test]
fn session_contract_has_query_and_resolution_without_resource_noise() {
    let input = serde_json::to_value(SessionInput {
        session_id: "session-1",
    })
    .unwrap();
    let output = serde_json::to_value(SessionOutput {
        found: true,
        story_id: Some("story-1"),
    })
    .unwrap();

    assert_eq!(input, serde_json::json!({"session_id": "session-1"}));
    assert_eq!(output, serde_json::json!({"found": true, "story_id": "story-1"}));
}

#[test]
fn validation_contract_never_contains_raw_idempotency_key() {
    let input = serde_json::to_value(ValidationInput {
        player_contribution_bytes: 7,
        player_contribution_sha256: digest("player"),
        idempotency_key_present: true,
    })
    .unwrap();
    let serialized = serde_json::to_string(&input).unwrap();

    assert!(input["idempotency_key_present"].as_bool().unwrap());
    assert!(!serialized.contains("secret-idempotency-key"));
}
