use super::*;
use crate::pipeline::common::{ProposalCheckCriterion, ProposalScoreCriterion};
use serde_json::{Value, json};

fn config() -> ValidateConfig {
    ValidateConfig {
        checks: vec![
            ProposalCheckCriterion {
                name: "logic".to_owned(),
                description: "Logical consistency.".to_owned(),
            },
            ProposalCheckCriterion {
                name: "constraints".to_owned(),
                description: "Constraint compliance.".to_owned(),
            },
        ],
        scores: vec![
            ProposalScoreCriterion {
                name: "goal".to_owned(),
                description: "Goal completion.".to_owned(),
                min_score: 0.0,
                max_score: 10.0,
                target_score: 8.0,
                target_guidance: "Advance the goal.".to_owned(),
            },
            ProposalScoreCriterion {
                name: "quality".to_owned(),
                description: "Narrative quality.".to_owned(),
                min_score: 1.0,
                max_score: 5.0,
                target_score: 4.0,
                target_guidance: "Use clear prose.".to_owned(),
            },
        ],
        max_issue_bytes: 32,
    }
}

fn valid_output() -> Value {
    json!({
        "checks": [
            {"name": "logic", "passed": true, "issue": null},
            {"name": "constraints", "passed": false, "issue": "Remove the forbidden outcome."}
        ],
        "scores": [
            {"name": "goal", "score": 8.0, "issue": null},
            {"name": "quality", "score": 3.0, "issue": "Clarify the final transition."}
        ]
    })
}

fn parse_value(value: &Value) -> Result<ValidationReport, PipelineError> {
    parse_validation_report(&value.to_string(), 7, &config())
}

fn error_code(error: PipelineError) -> &'static str {
    match error {
        PipelineError::InvalidOutput { code, .. } => code,
        _ => panic!("expected invalid output"),
    }
}

fn score_only_config() -> ValidateConfig {
    ValidateConfig {
        checks: Vec::new(),
        scores: vec![ProposalScoreCriterion {
            name: "score".to_owned(),
            description: "Score criterion.".to_owned(),
            min_score: 0.0,
            max_score: 10.0,
            target_score: 5.0,
            target_guidance: "Meet the target.".to_owned(),
        }],
        max_issue_bytes: 8,
    }
}

fn parse_score(score: f64, issue: Option<&str>) -> Result<ValidationReport, PipelineError> {
    let value = json!({
        "checks": [],
        "scores": [{"name": "score", "score": score, "issue": issue}]
    });
    parse_validation_report(&value.to_string(), 1, &score_only_config())
}

#[test]
fn valid_reordered_items_are_materialized_in_configured_order() {
    let value = json!({
        "checks": [
            {"name": "constraints", "passed": false, "issue": "  Remove the forbidden outcome.  "},
            {"name": "logic", "passed": true, "issue": null}
        ],
        "scores": [
            {"name": "quality", "score": 3.0, "issue": "Clarify the final transition."},
            {"name": "goal", "score": 8.0, "issue": null}
        ]
    });

    let report = parse_value(&value).expect("valid report");

    assert_eq!(report.proposal_version, 7);
    assert_eq!(
        report.checks.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
        ["logic", "constraints"]
    );
    assert_eq!(
        report.scores.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
        ["goal", "quality"]
    );
    assert_eq!(
        report.checks[1].issue.as_ref().map(|issue| issue.as_str()),
        Some("Remove the forbidden outcome.")
    );
}

#[test]
fn invalid_json_is_rejected() {
    let error = parse_validation_report("not json", 1, &config()).expect_err("invalid JSON fails");

    assert_eq!(error_code(error), "validate_output_invalid_json");
}

#[test]
fn unknown_fields_are_rejected_as_invalid_json() {
    let mut value = valid_output();
    value["decision"] = json!("pass");

    let error = parse_value(&value).expect_err("unknown field fails");

    assert_eq!(error_code(error), "validate_output_invalid_json");
}

#[test]
fn missing_check_is_rejected_by_count() {
    let mut value = valid_output();
    value["checks"].as_array_mut().expect("checks array").pop();

    let error = parse_value(&value).expect_err("missing check fails");

    assert_eq!(error_code(error), "validate_check_count_mismatch");
}

#[test]
fn missing_score_is_rejected_by_count() {
    let mut value = valid_output();
    value["scores"].as_array_mut().expect("scores array").pop();

    let error = parse_value(&value).expect_err("missing score fails");

    assert_eq!(error_code(error), "validate_score_count_mismatch");
}

#[test]
fn duplicate_check_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["name"] = json!("logic");

    let error = parse_value(&value).expect_err("duplicate fails");

    assert_eq!(error_code(error), "validate_criterion_duplicate");
}

#[test]
fn duplicate_score_is_rejected() {
    let mut value = valid_output();
    value["scores"][1]["name"] = json!("goal");

    let error = parse_value(&value).expect_err("duplicate fails");

    assert_eq!(error_code(error), "validate_criterion_duplicate");
}

#[test]
fn unknown_criterion_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["name"] = json!("unknown");

    let error = parse_value(&value).expect_err("unknown fails");

    assert_eq!(error_code(error), "validate_criterion_unknown");
}

#[test]
fn wrong_criterion_kind_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["name"] = json!("goal");

    let error = parse_value(&value).expect_err("wrong kind fails");

    assert_eq!(error_code(error), "validate_criterion_kind_mismatch");
}

#[test]
fn non_finite_score_is_rejected() {
    let raw = r#"{"checks":[],"scores":[{"name":"score","score":1e999,"issue":null}]}"#;

    let error = parse_validation_report(raw, 1, &score_only_config()).expect_err("non-finite score fails");

    assert_eq!(error_code(error), "validate_score_non_finite");
}

#[test]
fn nan_score_is_rejected() {
    let raw = r#"{"checks":[],"scores":[{"name":"score","score":NaN,"issue":null}]}"#;

    let error = parse_validation_report(raw, 1, &score_only_config()).expect_err("NaN score fails");

    assert_eq!(error_code(error), "validate_score_non_finite");
}

#[test]
fn positive_infinity_score_is_rejected() {
    let raw = r#"{"checks":[],"scores":[{"name":"score","score":Infinity,"issue":null}]}"#;

    let error = parse_validation_report(raw, 1, &score_only_config()).expect_err("infinite score fails");

    assert_eq!(error_code(error), "validate_score_non_finite");
}

#[test]
fn negative_infinity_score_is_rejected() {
    let raw = r#"{"checks":[],"scores":[{"name":"score","score":-Infinity,"issue":null}]}"#;

    let error = parse_validation_report(raw, 1, &score_only_config()).expect_err("infinite score fails");

    assert_eq!(error_code(error), "validate_score_non_finite");
}

#[test]
fn score_below_range_is_rejected() {
    let error = parse_score(-0.1, Some("too low")).expect_err("out of range fails");

    assert_eq!(error_code(error), "validate_score_out_of_range");
}

#[test]
fn score_above_range_is_rejected() {
    let error = parse_score(10.1, None).expect_err("out of range fails");

    assert_eq!(error_code(error), "validate_score_out_of_range");
}

#[test]
fn score_at_minimum_is_accepted() {
    let report = parse_score(0.0, Some("Improve.")).expect("minimum is valid");

    assert_eq!(report.scores[0].score, 0.0);
}

#[test]
fn score_at_target_is_accepted_without_issue() {
    let report = parse_score(5.0, None).expect("target is valid");

    assert_eq!(report.scores[0].score, 5.0);
}

#[test]
fn score_at_maximum_is_accepted_without_issue() {
    let report = parse_score(10.0, None).expect("maximum is valid");

    assert_eq!(report.scores[0].score, 10.0);
}

#[test]
fn failed_check_with_null_issue_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["issue"] = Value::Null;

    let error = parse_value(&value).expect_err("missing issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn failed_check_with_empty_issue_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["issue"] = json!("");

    let error = parse_value(&value).expect_err("empty issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn failed_check_with_whitespace_issue_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["issue"] = json!("   ");

    let error = parse_value(&value).expect_err("whitespace issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn failed_check_with_oversized_issue_is_rejected() {
    let mut value = valid_output();
    value["checks"][1]["issue"] = json!("x".repeat(33));

    let error = parse_value(&value).expect_err("oversized issue fails");

    assert_eq!(error_code(error), "validate_issue_too_long");
}

#[test]
fn passing_check_with_non_null_issue_is_rejected() {
    let mut value = valid_output();
    value["checks"][0]["issue"] = json!("");

    let error = parse_value(&value).expect_err("unexpected issue fails");

    assert_eq!(error_code(error), "validate_issue_unexpected");
}

#[test]
fn unmet_score_with_null_issue_is_rejected() {
    let error = parse_score(4.0, None).expect_err("missing issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn unmet_score_with_empty_issue_is_rejected() {
    let error = parse_score(4.0, Some("")).expect_err("empty issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn unmet_score_with_whitespace_issue_is_rejected() {
    let error = parse_score(4.0, Some("   ")).expect_err("whitespace issue fails");

    assert_eq!(error_code(error), "validate_issue_missing");
}

#[test]
fn unmet_score_with_oversized_issue_is_rejected() {
    let error = parse_score(4.0, Some("123456789")).expect_err("oversized issue fails");

    assert_eq!(error_code(error), "validate_issue_too_long");
}

#[test]
fn met_score_with_non_null_issue_is_rejected() {
    let error = parse_score(5.0, Some("")).expect_err("unexpected issue fails");

    assert_eq!(error_code(error), "validate_issue_unexpected");
}
