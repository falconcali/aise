use super::*;

#[test]
fn retrieval_hint_domain_bound_is_uniform() {
    let at_limit = "x".repeat(RetrievalHint::MAX_BYTES);
    let over_limit = "x".repeat(RetrievalHint::MAX_BYTES + 1);

    let fact_hint = RetrievalHint::try_new(at_limit.clone()).expect("fact-side hint at limit must succeed");
    let rumor_hint = RetrievalHint::try_new(at_limit).expect("rumor-side hint at limit must succeed");
    assert_eq!(fact_hint.as_str().len(), rumor_hint.as_str().len());
    assert_eq!(fact_hint.as_str().len(), RetrievalHint::MAX_BYTES);

    let fact_overflow = RetrievalHint::try_new(over_limit.clone()).unwrap_err();
    let rumor_overflow = RetrievalHint::try_new(over_limit).unwrap_err();
    assert!(matches!(
        fact_overflow,
        RetrievalHintError::TooLong { maximum, .. } if maximum == RetrievalHint::MAX_BYTES as u64
    ));
    assert!(matches!(
        rumor_overflow,
        RetrievalHintError::TooLong { maximum, .. } if maximum == RetrievalHint::MAX_BYTES as u64
    ));

    assert!(matches!(RetrievalHint::try_new("").unwrap_err(), RetrievalHintError::Empty));
    assert!(matches!(RetrievalHint::try_new("   ").unwrap_err(), RetrievalHintError::Empty));
}
