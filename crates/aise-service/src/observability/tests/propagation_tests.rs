use super::*;

#[test]
fn masker_redacts_credentials_and_pii_within_bound() {
    let masker = StreamingMasker::new(256, true);
    let masked =
        masker.mask("authorization=Bearer token secret_key=sk-lf-secret email=user@example.com phone=13800138000");

    assert!(!masked.contains("sk-lf-secret"));
    assert!(!masked.contains("user@example.com"));
    assert!(!masked.contains("13800138000"));
    assert!(masked.contains(REDACTED));
    assert!(masked.len() <= 256);
}
