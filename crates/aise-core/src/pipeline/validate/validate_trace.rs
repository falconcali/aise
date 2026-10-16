use crate::pipeline::common::{PipelineError, ValidationReport};
use crate::pipeline::validate::ValidateInput;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub(super) fn begin_evaluator(observation: &Observation, input: &ValidateInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "validate_evaluator",
        kind: ObservationKind::Evaluator,
        input: observation.capture_content(&(input.proposal_version, input.proposal)),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_evaluator(observation: Observation, result: &Result<ValidationReport, PipelineError>) {
    let outcome = match result {
        Ok(report) => ObservationOutcome {
            status: ObservationStatus::Ok,
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
