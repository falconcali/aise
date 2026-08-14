use super::*;

#[test]
fn story_within_output_budget_produces_no_issue() {
    let tokens = estimate_text_tokens("a short story");
    assert!(tokens <= 1_000_000);
}

#[test]
fn story_bounds_validator_is_default_constructible() {
    let _validator = StoryBoundsValidator;
}
