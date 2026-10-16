use crate::pipeline::common::PipelineError;
use crate::pipeline::runtime::runtime_validation::ValidationLoopResult;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus, Trace,
};

pub(super) fn begin_validation_loop(trace: &Trace, initial_proposal: &str) -> Observation {
    trace.begin_observation(ObservationSpec {
        name: "validation_loop",
        kind: ObservationKind::Span,
        input: trace.capture_content(&initial_proposal),
        metadata: Vec::new(),
    })
}

pub(super) fn finish_validation_loop(observation: Observation, result: &Result<ValidationLoopResult, PipelineError>) {
    let outcome = match result {
        Ok(result) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&(result.proposal.as_str(), &result.report)),
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
#[path = "tests/runtime_trace_tests.rs"]
mod tests;
