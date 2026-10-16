use crate::core::BoundedText;
use crate::pipeline::common::{
    PipelineError, PipelineStage, ValidationCheckResult, ValidationReport, ValidationScoreResult,
};
use crate::pipeline::validate::ValidateConfig;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateLlmOutputDto {
    checks: Vec<ValidateCheckDto>,
    scores: Vec<ValidateScoreDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateCheckDto {
    name: String,
    passed: bool,
    issue: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateScoreDto {
    name: String,
    score: f64,
    issue: Option<String>,
}

pub(super) fn parse_validation_report(
    raw: &str,
    proposal_version: u32,
    config: &ValidateConfig,
) -> Result<ValidationReport, PipelineError> {
    let output = serde_json::from_str::<ValidateLlmOutputDto>(raw).map_err(|error| {
        let code = if error.to_string().contains("number out of range") || contains_non_finite_score_token(raw) {
            "validate_score_non_finite"
        } else {
            "validate_output_invalid_json"
        };
        invalid_output(
            code,
            format!("invalid validation JSON at line {} column {}", error.line(), error.column()),
        )
    })?;
    validate_returned_names(&output, config)?;
    if output.checks.len() != config.checks.len() {
        return Err(invalid_output(
            "validate_check_count_mismatch",
            format!(
                "returned check count {} does not match configured count {}",
                output.checks.len(),
                config.checks.len()
            ),
        ));
    }
    if output.scores.len() != config.scores.len() {
        return Err(invalid_output(
            "validate_score_count_mismatch",
            format!(
                "returned score count {} does not match configured count {}",
                output.scores.len(),
                config.scores.len()
            ),
        ));
    }

    let mut checks = output
        .checks
        .into_iter()
        .map(|check| (check.name.clone(), check))
        .collect::<HashMap<_, _>>();
    let mut scores = output
        .scores
        .into_iter()
        .map(|score| (score.name.clone(), score))
        .collect::<HashMap<_, _>>();
    let checks = config
        .checks
        .iter()
        .map(|criterion| {
            let result = checks.remove(&criterion.name).ok_or_else(|| {
                invalid_output(
                    "validate_check_count_mismatch",
                    format!("configured check {} is missing", criterion.name),
                )
            })?;
            let issue = validate_issue(&result.name, result.issue, !result.passed, config.max_issue_bytes)?;
            Ok(ValidationCheckResult {
                name: criterion.name.clone(),
                description: criterion.description.clone(),
                passed: result.passed,
                issue,
            })
        })
        .collect::<Result<Vec<_>, PipelineError>>()?;
    let scores = config
        .scores
        .iter()
        .map(|criterion| {
            let result = scores.remove(&criterion.name).ok_or_else(|| {
                invalid_output(
                    "validate_score_count_mismatch",
                    format!("configured score {} is missing", criterion.name),
                )
            })?;
            if !result.score.is_finite() {
                return Err(invalid_output(
                    "validate_score_non_finite",
                    format!("score {} must be finite", result.name),
                ));
            }
            if result.score < criterion.min_score || result.score > criterion.max_score {
                return Err(invalid_output(
                    "validate_score_out_of_range",
                    format!(
                        "score {} is outside configured range [{}, {}]",
                        result.name, criterion.min_score, criterion.max_score
                    ),
                ));
            }
            let issue = validate_issue(
                &result.name,
                result.issue,
                result.score < criterion.target_score,
                config.max_issue_bytes,
            )?;
            Ok(ValidationScoreResult {
                name: criterion.name.clone(),
                description: criterion.description.clone(),
                min_score: criterion.min_score,
                max_score: criterion.max_score,
                target_score: criterion.target_score,
                score: result.score,
                issue,
            })
        })
        .collect::<Result<Vec<_>, PipelineError>>()?;

    Ok(ValidationReport {
        proposal_version,
        checks,
        scores,
    })
}

fn validate_returned_names(output: &ValidateLlmOutputDto, config: &ValidateConfig) -> Result<(), PipelineError> {
    let configured_checks = config.checks.iter().map(|check| check.name.as_str()).collect::<HashSet<_>>();
    let configured_scores = config.scores.iter().map(|score| score.name.as_str()).collect::<HashSet<_>>();
    let mut seen_checks = HashSet::with_capacity(output.checks.len());
    for check in &output.checks {
        if !seen_checks.insert(check.name.as_str()) {
            return Err(invalid_output(
                "validate_criterion_duplicate",
                format!("check {} appears more than once", check.name),
            ));
        }
        validate_name_kind(&check.name, &configured_checks, &configured_scores, "check")?;
    }
    let mut seen_scores = HashSet::with_capacity(output.scores.len());
    for score in &output.scores {
        if !seen_scores.insert(score.name.as_str()) {
            return Err(invalid_output(
                "validate_criterion_duplicate",
                format!("score {} appears more than once", score.name),
            ));
        }
        validate_name_kind(&score.name, &configured_scores, &configured_checks, "score")?;
    }
    Ok(())
}

fn validate_name_kind(
    name: &str,
    matching_kind: &HashSet<&str>,
    opposite_kind: &HashSet<&str>,
    returned_kind: &str,
) -> Result<(), PipelineError> {
    if matching_kind.contains(name) {
        return Ok(());
    }
    if opposite_kind.contains(name) {
        return Err(invalid_output(
            "validate_criterion_kind_mismatch",
            format!("{returned_kind} {name} is configured with the opposite criterion kind"),
        ));
    }
    Err(invalid_output(
        "validate_criterion_unknown",
        format!("{returned_kind} {name} is not configured"),
    ))
}

fn validate_issue(
    name: &str,
    issue: Option<String>,
    required: bool,
    max_issue_bytes: usize,
) -> Result<Option<BoundedText>, PipelineError> {
    match (required, issue) {
        (false, None) => Ok(None),
        (false, Some(_)) => Err(invalid_output(
            "validate_issue_unexpected",
            format!("criterion {name} must not include an issue"),
        )),
        (true, None) => Err(invalid_output(
            "validate_issue_missing",
            format!("criterion {name} requires an issue"),
        )),
        (true, Some(issue)) => {
            let issue = issue.trim();
            if issue.is_empty() {
                return Err(invalid_output(
                    "validate_issue_missing",
                    format!("criterion {name} requires a non-empty issue"),
                ));
            }
            if issue.len() > max_issue_bytes {
                return Err(invalid_output(
                    "validate_issue_too_long",
                    format!("criterion {name} issue exceeds {max_issue_bytes} bytes"),
                ));
            }
            BoundedText::try_new(issue, "validation_issue", max_issue_bytes)
                .map(Some)
                .map_err(|_| {
                    invalid_output(
                        "validate_issue_too_long",
                        format!("criterion {name} issue exceeds {max_issue_bytes} bytes"),
                    )
                })
        }
    }
}

fn contains_non_finite_score_token(raw: &str) -> bool {
    raw.match_indices("\"score\"").any(|(index, _)| {
        raw[index + "\"score\"".len()..]
            .split_once(':')
            .map(|(_, value)| {
                let value = value.trim_start();
                value.starts_with("NaN") || value.starts_with("Infinity") || value.starts_with("-Infinity")
            })
            .unwrap_or(false)
    })
}

fn invalid_output(code: &'static str, message: impl Into<String>) -> PipelineError {
    PipelineError::invalid_output(PipelineStage::Validate, code, message)
}

#[cfg(test)]
#[path = "tests/validate_report_tests.rs"]
mod tests;
