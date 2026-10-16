use crate::pipeline::common::{PipelineError, ValidationReport};
use crate::pipeline::validate::ValidateInput;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const PROPOSAL_VERSION: &str = "aise.validate.proposal_version";
const CHECKS_TOTAL: &str = "aise.validate.checks.total";
const CHECKS_PASSED: &str = "aise.validate.checks.passed";
const CHECKS_FAILED: &str = "aise.validate.checks.failed";
const SCORES_TOTAL: &str = "aise.validate.scores.total";
const SCORES_MET: &str = "aise.validate.scores.met";
const SCORES_UNMET: &str = "aise.validate.scores.unmet";
const REPAIR_ROUNDS_USED: &str = "aise.validate.repair_rounds_used";
const REPAIR_ROUNDS_REMAINING: &str = "aise.validate.repair_rounds_remaining";
const CHECK_NAMES: [&str; 16] = [
    "aise.validate.check.0.name",
    "aise.validate.check.1.name",
    "aise.validate.check.2.name",
    "aise.validate.check.3.name",
    "aise.validate.check.4.name",
    "aise.validate.check.5.name",
    "aise.validate.check.6.name",
    "aise.validate.check.7.name",
    "aise.validate.check.8.name",
    "aise.validate.check.9.name",
    "aise.validate.check.10.name",
    "aise.validate.check.11.name",
    "aise.validate.check.12.name",
    "aise.validate.check.13.name",
    "aise.validate.check.14.name",
    "aise.validate.check.15.name",
];
const CHECK_PASSED: [&str; 16] = [
    "aise.validate.check.0.passed",
    "aise.validate.check.1.passed",
    "aise.validate.check.2.passed",
    "aise.validate.check.3.passed",
    "aise.validate.check.4.passed",
    "aise.validate.check.5.passed",
    "aise.validate.check.6.passed",
    "aise.validate.check.7.passed",
    "aise.validate.check.8.passed",
    "aise.validate.check.9.passed",
    "aise.validate.check.10.passed",
    "aise.validate.check.11.passed",
    "aise.validate.check.12.passed",
    "aise.validate.check.13.passed",
    "aise.validate.check.14.passed",
    "aise.validate.check.15.passed",
];
const SCORE_NAMES: [&str; 16] = [
    "aise.validate.score.0.name",
    "aise.validate.score.1.name",
    "aise.validate.score.2.name",
    "aise.validate.score.3.name",
    "aise.validate.score.4.name",
    "aise.validate.score.5.name",
    "aise.validate.score.6.name",
    "aise.validate.score.7.name",
    "aise.validate.score.8.name",
    "aise.validate.score.9.name",
    "aise.validate.score.10.name",
    "aise.validate.score.11.name",
    "aise.validate.score.12.name",
    "aise.validate.score.13.name",
    "aise.validate.score.14.name",
    "aise.validate.score.15.name",
];
const SCORE_VALUES: [&str; 16] = [
    "aise.validate.score.0.value",
    "aise.validate.score.1.value",
    "aise.validate.score.2.value",
    "aise.validate.score.3.value",
    "aise.validate.score.4.value",
    "aise.validate.score.5.value",
    "aise.validate.score.6.value",
    "aise.validate.score.7.value",
    "aise.validate.score.8.value",
    "aise.validate.score.9.value",
    "aise.validate.score.10.value",
    "aise.validate.score.11.value",
    "aise.validate.score.12.value",
    "aise.validate.score.13.value",
    "aise.validate.score.14.value",
    "aise.validate.score.15.value",
];
const SCORE_TARGETS: [&str; 16] = [
    "aise.validate.score.0.target",
    "aise.validate.score.1.target",
    "aise.validate.score.2.target",
    "aise.validate.score.3.target",
    "aise.validate.score.4.target",
    "aise.validate.score.5.target",
    "aise.validate.score.6.target",
    "aise.validate.score.7.target",
    "aise.validate.score.8.target",
    "aise.validate.score.9.target",
    "aise.validate.score.10.target",
    "aise.validate.score.11.target",
    "aise.validate.score.12.target",
    "aise.validate.score.13.target",
    "aise.validate.score.14.target",
    "aise.validate.score.15.target",
];

pub(super) fn begin_evaluator(observation: &Observation, input: &ValidateInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "validate_evaluator",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(input.proposal_version, input.proposal)),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_evaluator(
    observation: Observation,
    input: &ValidateInput<'_>,
    result: &Result<ValidationReport, PipelineError>,
) {
    let outcome = match result {
        Ok(report) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: validation_metadata(input, report),
            output: observation.capture_content(report),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(observation_error(error)),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}

fn validation_metadata(input: &ValidateInput<'_>, report: &ValidationReport) -> Vec<Attribute> {
    let checks_passed = report.checks.iter().filter(|check| check.passed).count();
    let scores_met = report.scores.iter().filter(|score| score.score >= score.target_score).count();
    let mut metadata = vec![
        Attribute::u64(PROPOSAL_VERSION, input.proposal_version as u64),
        Attribute::u64(CHECKS_TOTAL, report.checks.len() as u64),
        Attribute::u64(CHECKS_PASSED, checks_passed as u64),
        Attribute::u64(CHECKS_FAILED, report.checks.len().saturating_sub(checks_passed) as u64),
        Attribute::u64(SCORES_TOTAL, report.scores.len() as u64),
        Attribute::u64(SCORES_MET, scores_met as u64),
        Attribute::u64(SCORES_UNMET, report.scores.len().saturating_sub(scores_met) as u64),
        Attribute::u64(REPAIR_ROUNDS_USED, input.repair_rounds_used as u64),
        Attribute::u64(REPAIR_ROUNDS_REMAINING, input.repair_rounds_remaining as u64),
    ];
    for (index, check) in report.checks.iter().enumerate() {
        metadata.push(Attribute::string(CHECK_NAMES[index], check.name.clone()));
        metadata.push(Attribute::bool(CHECK_PASSED[index], check.passed));
    }
    for (index, score) in report.scores.iter().enumerate() {
        metadata.push(Attribute::string(SCORE_NAMES[index], score.name.clone()));
        metadata.push(Attribute::f64(SCORE_VALUES[index], score.score));
        metadata.push(Attribute::f64(SCORE_TARGETS[index], score.target_score));
    }
    metadata
}

fn observation_error(error: &PipelineError) -> ObservationError {
    let (code, failure_kind) = match error {
        PipelineError::InvalidOutput { code, .. } => (*code, "invalid_output"),
        PipelineError::InconsistentData { code, .. } => (*code, "inconsistent_data"),
        PipelineError::Llm { source, .. } => (source.kind(), "llm"),
        PipelineError::Prompt { .. } => ("prompt_error", "prompt"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Narrative { .. } => ("narrative_error", "narrative"),
        PipelineError::EmptyOutput { .. } => ("empty_output", "pipeline"),
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
    };
    ObservationError {
        code: code.to_owned(),
        failure_kind: failure_kind.to_owned(),
        stage: Some("validate".to_owned()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/validate_trace_tests.rs"]
mod tests;
