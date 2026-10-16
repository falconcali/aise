use super::*;

fn assert_code(config: ValidateConfig, expected_code: &'static str) {
    let error = config.validate().expect_err("config should be invalid");
    assert!(matches!(
        error,
        PipelineError::InvalidConfig {
            stage: PipelineStage::Validate,
            code,
            ..
        } if code == expected_code
    ));
}

fn check(name: impl Into<String>, description: impl Into<String>) -> ProposalCheckCriterion {
    ProposalCheckCriterion {
        name: name.into(),
        description: description.into(),
    }
}

fn score(name: impl Into<String>) -> ProposalScoreCriterion {
    ProposalScoreCriterion {
        name: name.into(),
        description: "observable score".to_owned(),
        min_score: 0.0,
        max_score: 10.0,
        target_score: 8.0,
        target_guidance: "Meet the observable target.".to_owned(),
    }
}

#[test]
fn default_config_has_stable_criteria_and_limits() {
    let config = ValidateConfig::default();

    assert_eq!(
        config
            .checks
            .iter()
            .map(|criterion| criterion.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "internal_logic",
            "narrative_consistency",
            "explicit_constraints",
            "private_knowledge_boundary"
        ]
    );
    assert_eq!(
        config
            .scores
            .iter()
            .map(|criterion| criterion.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "story_goal_completion",
            "player_response",
            "character_consistency",
            "narrative_quality"
        ]
    );
    assert!(
        config.scores.iter().all(|criterion| criterion.min_score == 0.0
            && criterion.max_score == 10.0
            && criterion.target_score == 8.0)
    );
    assert_eq!(
        config
            .scores
            .iter()
            .map(|criterion| criterion.target_guidance.as_str())
            .collect::<Vec<_>>(),
        [
            "The segment makes visible, causally grounded progress toward the immediate story goal without forcing completion unsupported by the evidence.",
            "The segment realizes the pending player contribution in the prose, preserves its essential meaning, and does not invent an unprovided consequential player choice.",
            "Character actions, knowledge, voice, and immediate intent remain consistent with supplied role evidence and character decisions unless visible events justify a change.",
            "The segment uses clear causal and temporal transitions, readable prose, and a natural interaction boundary without relying on abrupt or unsupported developments."
        ]
    );
    assert_eq!(config.max_issue_bytes, 2048);
    config.validate().expect("default config should be valid");
}

#[test]
fn criteria_cannot_both_be_empty() {
    let config = ValidateConfig {
        checks: Vec::new(),
        scores: Vec::new(),
        max_issue_bytes: 1,
    };

    assert_code(config, "validate_criteria_empty");
}

#[test]
fn check_count_enforces_upper_bound() {
    let config = ValidateConfig {
        checks: (0..=MAX_VALIDATE_CHECKS)
            .map(|index| check(format!("check_{index}"), "observable check"))
            .collect(),
        ..ValidateConfig::default()
    };

    assert_code(config, "validate_checks_too_many");
}

#[test]
fn score_count_enforces_upper_bound() {
    let config = ValidateConfig {
        scores: (0..=MAX_VALIDATE_SCORES).map(|index| score(format!("score_{index}"))).collect(),
        ..ValidateConfig::default()
    };

    assert_code(config, "validate_scores_too_many");
}

#[test]
fn criterion_name_enforces_nonempty_and_byte_limit() {
    let mut empty = ValidateConfig::default();
    empty.checks[0].name.clear();
    assert_code(empty, "validate_criterion_name_empty");

    let mut boundary = ValidateConfig::default();
    boundary.checks[0].name = "a".repeat(MAX_CRITERION_NAME_BYTES);
    boundary.validate().expect("boundary name should be valid");

    let mut oversized = ValidateConfig::default();
    oversized.checks[0].name = "a".repeat(MAX_CRITERION_NAME_BYTES + 1);
    assert_code(oversized, "validate_criterion_name_too_long");
}

#[test]
fn criterion_description_enforces_trimmed_nonempty_and_byte_limit() {
    let mut empty = ValidateConfig::default();
    empty.checks[0].description = " \n ".to_owned();
    assert_code(empty, "validate_criterion_description_empty");

    let mut boundary = ValidateConfig::default();
    boundary.checks[0].description = format!(" {} ", "a".repeat(MAX_CRITERION_DESCRIPTION_BYTES));
    boundary.validate().expect("boundary description should be valid");

    let mut oversized = ValidateConfig::default();
    oversized.checks[0].description = "a".repeat(MAX_CRITERION_DESCRIPTION_BYTES + 1);
    assert_code(oversized, "validate_criterion_description_too_long");
}

#[test]
fn criterion_names_are_unique_across_kinds() {
    let mut config = ValidateConfig::default();
    config.scores[0].name = config.checks[0].name.clone();

    assert_code(config, "validate_criterion_name_duplicate");
}

#[test]
fn score_values_must_be_finite() {
    for update in [
        (|criterion: &mut ProposalScoreCriterion| criterion.min_score = f64::NAN) as fn(&mut ProposalScoreCriterion),
        |criterion: &mut ProposalScoreCriterion| criterion.max_score = f64::INFINITY,
        |criterion: &mut ProposalScoreCriterion| criterion.target_score = f64::NEG_INFINITY,
    ] {
        let mut config = ValidateConfig::default();
        update(&mut config.scores[0]);
        assert_code(config, "validate_score_value_non_finite");
    }
}

#[test]
fn score_range_and_target_must_be_ordered() {
    let mut equal_range = ValidateConfig::default();
    equal_range.scores[0].min_score = equal_range.scores[0].max_score;
    assert_code(equal_range, "validate_score_range_invalid");

    let mut target_below = ValidateConfig::default();
    target_below.scores[0].target_score = target_below.scores[0].min_score - 1.0;
    assert_code(target_below, "validate_score_range_invalid");

    let mut target_above = ValidateConfig::default();
    target_above.scores[0].target_score = target_above.scores[0].max_score + 1.0;
    assert_code(target_above, "validate_score_range_invalid");
}

#[test]
fn score_target_accepts_both_range_boundaries() {
    let mut minimum = ValidateConfig::default();
    minimum.scores[0].target_score = minimum.scores[0].min_score;
    minimum.validate().expect("minimum target should be valid");

    let mut maximum = ValidateConfig::default();
    maximum.scores[0].target_score = maximum.scores[0].max_score;
    maximum.validate().expect("maximum target should be valid");
}

#[test]
fn score_target_guidance_rejects_empty_and_whitespace_only_values() {
    let mut empty = ValidateConfig::default();
    empty.scores[0].target_guidance.clear();
    assert_code(empty, "validate_score_target_guidance_empty");

    let mut whitespace = ValidateConfig::default();
    whitespace.scores[0].target_guidance = " \n ".to_owned();
    assert_code(whitespace, "validate_score_target_guidance_empty");
}

#[test]
fn score_target_guidance_enforces_trimmed_byte_limit() {
    let mut boundary = ValidateConfig::default();
    boundary.scores[0].target_guidance = format!(" {} ", "a".repeat(MAX_TARGET_GUIDANCE_BYTES));
    boundary.validate().expect("boundary target guidance should be valid");

    let mut oversized = ValidateConfig::default();
    oversized.scores[0].target_guidance = "a".repeat(MAX_TARGET_GUIDANCE_BYTES + 1);
    assert_code(oversized, "validate_score_target_guidance_too_long");
}

#[test]
fn quality_policy_clones_validated_criteria_in_order() {
    let config = ValidateConfig::default();
    config.validate().expect("default config should be valid");

    let policy = config.quality_policy();

    assert_eq!(policy.checks(), config.checks.as_slice());
    assert_eq!(policy.scores(), config.scores.as_slice());
}

#[test]
fn issue_bytes_enforce_inclusive_bounds() {
    let minimum = ValidateConfig {
        max_issue_bytes: 1,
        ..ValidateConfig::default()
    };
    minimum.validate().expect("minimum issue bytes should be valid");

    let maximum = ValidateConfig {
        max_issue_bytes: MAX_ISSUE_BYTES_LIMIT,
        ..ValidateConfig::default()
    };
    maximum.validate().expect("maximum issue bytes should be valid");

    let zero = ValidateConfig {
        max_issue_bytes: 0,
        ..ValidateConfig::default()
    };
    assert_code(zero, "validate_max_issue_bytes_invalid");

    let excessive = ValidateConfig {
        max_issue_bytes: MAX_ISSUE_BYTES_LIMIT + 1,
        ..ValidateConfig::default()
    };
    assert_code(excessive, "validate_max_issue_bytes_invalid");
}

#[test]
fn config_deserialization_rejects_unknown_fields() {
    let result = serde_json::from_value::<ValidateConfig>(serde_json::json!({
        "checks": [],
        "scores": [],
        "max_issue_bytes": 1,
        "unknown": true
    }));

    assert!(result.is_err());
}

#[test]
fn config_deserialization_requires_score_target_guidance() {
    let result = serde_json::from_value::<ValidateConfig>(serde_json::json!({
        "checks": [],
        "scores": [{
            "name": "quality",
            "description": "Narrative quality.",
            "min_score": 0.0,
            "max_score": 10.0,
            "target_score": 8.0
        }],
        "max_issue_bytes": 1
    }));

    assert!(result.is_err());
}
