use crate::core::{CharacterDecision, RoleId};
use crate::llm::LlmError;
use crate::pipeline::common::PipelineError;
use crate::pipeline::think::ThinkOutput;
use crate::trace::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

const THINK_REQUESTS: &str = "aise.think.requests";
const THINK_DECISIONS: &str = "aise.think.decisions";
const THINK_ROLE_ID: &str = "aise.think.role_id";

pub fn begin_think(parent: &Observation, request_count: usize) -> Observation {
    parent.begin(ObservationSpec {
        name: "think",
        kind: ObservationKind::Span,
        input: None,
        metadata: vec![Attribute::u64(THINK_REQUESTS, request_count as u64)],
    })
}

pub fn finish_think(observation: Observation, result: &Result<ThinkOutput, PipelineError>) {
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: vec![Attribute::u64(THINK_DECISIONS, output.decisions.len() as u64)],
            output: observation.capture_content(output),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            metadata: vec![Attribute::u64(THINK_DECISIONS, 0)],
            ..error_outcome(error)
        },
    };
    observation.finish(outcome);
}

pub fn begin_role(parent: &Observation, role_id: &RoleId) -> Observation {
    parent.begin(ObservationSpec {
        name: "think_role",
        kind: ObservationKind::Span,
        input: None,
        metadata: vec![Attribute::string(THINK_ROLE_ID, role_id.to_string())],
    })
}

pub fn finish_role(observation: Observation, result: &Result<CharacterDecision, PipelineError>) {
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(output),
            ..ObservationOutcome::default()
        },
        Err(error) => error_outcome(error),
    };
    observation.finish(outcome);
}

fn error_outcome(error: &PipelineError) -> ObservationOutcome {
    ObservationOutcome {
        status: error_status(error),
        error: Some(observation_error(error)),
        ..ObservationOutcome::default()
    }
}

fn error_status(error: &PipelineError) -> ObservationStatus {
    match error {
        PipelineError::Llm {
            source: LlmError::Cancelled,
            ..
        } => ObservationStatus::Cancelled,
        PipelineError::Llm {
            source: LlmError::Timeout,
            ..
        } => ObservationStatus::DeadlineExceeded,
        _ => ObservationStatus::Error,
    }
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
    };
    ObservationError {
        code: code.to_owned(),
        failure_kind: failure_kind.to_owned(),
        stage: Some("think".to_owned()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/think_trace_tests.rs"]
mod tests;
