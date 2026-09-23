use super::*;

#[test]
fn masks_authorization_cookie_and_secret_assignments() {
    let masker = StreamingMasker::new(1024, false);
    let value = r#"Authorization: Basic dXNlcjpwYXNz
cookie=session=top-secret
{"api_key":"api-secret","secret_key":"secret-value"}"#;

    let masked = masker.mask(value);

    assert!(!masked.contains("dXNlcjpwYXNz"));
    assert!(!masked.contains("top-secret"));
    assert!(!masked.contains("api-secret"));
    assert!(!masked.contains("secret-value"));
    assert!(masked.matches("[REDACTED]").count() >= 4);
}

#[test]
fn masks_langfuse_and_api_key_prefixes() {
    let masker = StreamingMasker::new(1024, false);
    let masked = masker.mask("one sk-abcdef123 two pk-lf-public123 three sk-lf-secret123");

    assert_eq!(masked, "one [REDACTED] two [REDACTED] three [REDACTED]");
}

#[test]
fn masks_secret_split_at_every_chunk_boundary() {
    let masker = StreamingMasker::new(1024, false);
    let value = "prefix Authorization: Bearer sensitive-token suffix";

    for boundary in 0..=value.len() {
        let masked = masker.mask_chunks([&value[..boundary], &value[boundary..]]);
        assert!(!masked.contains("sensitive-token"), "boundary {boundary}");
    }
}

#[test]
fn masking_happens_before_utf8_safe_truncation() {
    let masker = StreamingMasker::new(31, false);
    let value = "12345678901234567890 api_key=secret-value-that-crosses";

    let masked = masker.mask(value);

    assert!(masked.len() <= 31);
    assert!(!masked.contains("secret"));
    assert!(std::str::from_utf8(masked.as_bytes()).is_ok());
}

#[test]
fn redacted_policy_masks_email_and_phone() {
    let masker = StreamingMasker::new(1024, true);
    let masked = masker.mask("mail user@example.com phone +1-555-123-4567");

    assert!(!masked.contains("user@example.com"));
    assert!(!masked.contains("+1-555-123-4567"));
}

#[test]
fn full_content_mode_still_masks_credentials() {
    let masker = StreamingMasker::new(1024, false);
    let masked = masker.mask("password=hunter2");

    assert_eq!(masked, "password=[REDACTED]");
}
