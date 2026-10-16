use crate::pipeline::common::PipelineError;
use crate::pipeline::repair::RepairInput;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub(super) fn begin_repair(observation: &Observation, input: &RepairInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "repair_generation",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(input.proposal_version, input.proposal, input.issues)),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_repair(observation: Observation, result: &Result<String, PipelineError>) {
    let outcome = match result {
        Ok(proposal) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(proposal),
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
        stage: Some("repair".to_owned()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/repair_trace_tests.rs"]
mod tests;
