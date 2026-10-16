use crate::pipeline::common::{PipelineError, PipelineStage};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_VALIDATE_CHECKS: usize = 16;
pub const MAX_VALIDATE_SCORES: usize = 16;
pub const MAX_CRITERION_NAME_BYTES: usize = 64;
pub const MAX_CRITERION_DESCRIPTION_BYTES: usize = 2 * 1024;
pub const MAX_ISSUE_BYTES_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateCheckConfig {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateScoreConfig {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateConfig {
    pub checks: Vec<ValidateCheckConfig>,
    pub scores: Vec<ValidateScoreConfig>,
    pub max_issue_bytes: usize,
}

impl ValidateConfig {
    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.checks.is_empty() && self.scores.is_empty() {
            return Err(invalid_config(
                "validate_criteria_empty",
                "at least one validation criterion is required",
            ));
        }
        if self.checks.len() > MAX_VALIDATE_CHECKS {
            return Err(invalid_config(
                "validate_checks_too_many",
                format!("check count exceeds {MAX_VALIDATE_CHECKS}"),
            ));
        }
        if self.scores.len() > MAX_VALIDATE_SCORES {
            return Err(invalid_config(
                "validate_scores_too_many",
                format!("score count exceeds {MAX_VALIDATE_SCORES}"),
            ));
        }

        let mut names = HashSet::with_capacity(self.checks.len() + self.scores.len());
        for (index, check) in self.checks.iter().enumerate() {
            validate_text_fields(&check.name, &check.description, "check", index)?;
            if !names.insert(check.name.as_str()) {
                return Err(invalid_config(
                    "validate_criterion_name_duplicate",
                    format!("check criterion at index {index} has a duplicate name"),
                ));
            }
        }
        for (index, score) in self.scores.iter().enumerate() {
            validate_text_fields(&score.name, &score.description, "score", index)?;
            if !names.insert(score.name.as_str()) {
                return Err(invalid_config(
                    "validate_criterion_name_duplicate",
                    format!("score criterion at index {index} has a duplicate name"),
                ));
            }
            if !score.min_score.is_finite() || !score.max_score.is_finite() || !score.target_score.is_finite() {
                return Err(invalid_config(
                    "validate_score_value_non_finite",
                    format!("score criterion at index {index} contains a non-finite value"),
                ));
            }
            if score.min_score >= score.max_score
                || score.target_score < score.min_score
                || score.target_score > score.max_score
            {
                return Err(invalid_config(
                    "validate_score_range_invalid",
                    format!("score criterion at index {index} has an invalid range or target"),
                ));
            }
        }

        if !(1..=MAX_ISSUE_BYTES_LIMIT).contains(&self.max_issue_bytes) {
            return Err(invalid_config(
                "validate_max_issue_bytes_invalid",
                format!("max_issue_bytes must be between 1 and {MAX_ISSUE_BYTES_LIMIT}"),
            ));
        }
        Ok(())
    }
}

impl Default for ValidateConfig {
    fn default() -> Self {
        Self {
            checks: vec![
                check(
                    "internal_logic",
                    "The proposal contains no internal causal or temporal contradiction.",
                ),
                check(
                    "narrative_consistency",
                    "The proposal follows the current narrative state and direction.",
                ),
                check(
                    "explicit_constraints",
                    "The proposal satisfies every explicit story constraint.",
                ),
                check(
                    "private_knowledge_boundary",
                    "The proposal does not present private character knowledge as public fact.",
                ),
            ],
            scores: vec![
                score("story_goal_completion", "The proposal advances the immediate story goal."),
                score(
                    "player_response",
                    "The proposal responds directly to the player's contribution.",
                ),
                score(
                    "character_consistency",
                    "The proposal keeps each character consistent with the supplied role evidence.",
                ),
                score("narrative_quality", "The proposal is coherent and readable as story prose."),
            ],
            max_issue_bytes: 2048,
        }
    }
}

fn check(name: &str, description: &str) -> ValidateCheckConfig {
    ValidateCheckConfig {
        name: name.to_owned(),
        description: description.to_owned(),
    }
}

fn score(name: &str, description: &str) -> ValidateScoreConfig {
    ValidateScoreConfig {
        name: name.to_owned(),
        description: description.to_owned(),
        min_score: 0.0,
        max_score: 10.0,
        target_score: 8.0,
    }
}

fn validate_text_fields(
    name: &str,
    description: &str,
    criterion_kind: &str,
    index: usize,
) -> Result<(), PipelineError> {
    if name.is_empty() {
        return Err(invalid_config(
            "validate_criterion_name_empty",
            format!("{criterion_kind} criterion at index {index} has an empty name"),
        ));
    }
    if name.len() > MAX_CRITERION_NAME_BYTES {
        return Err(invalid_config(
            "validate_criterion_name_too_long",
            format!(
                "{criterion_kind} criterion at index {index} has a name exceeding {MAX_CRITERION_NAME_BYTES} bytes"
            ),
        ));
    }
    let description = description.trim();
    if description.is_empty() {
        return Err(invalid_config(
            "validate_criterion_description_empty",
            format!("{criterion_kind} criterion at index {index} has an empty description"),
        ));
    }
    if description.len() > MAX_CRITERION_DESCRIPTION_BYTES {
        return Err(invalid_config(
            "validate_criterion_description_too_long",
            format!(
                "{criterion_kind} criterion at index {index} has a description exceeding {MAX_CRITERION_DESCRIPTION_BYTES} bytes"
            ),
        ));
    }
    Ok(())
}

fn invalid_config(code: &'static str, message: impl Into<String>) -> PipelineError {
    PipelineError::invalid_config(PipelineStage::Validate, code, message)
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
