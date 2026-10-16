use crate::pipeline::common::PipelineError;
use crate::pipeline::retrieval::{RetrievalInput, RetrievalOutput};
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_retrieval(observation: &Observation, input: &RetrievalInput<'_>) -> Observation {
    observation.begin(ObservationSpec {
        name: "retrieve_knowledge",
        kind: ObservationKind::Retriever,
        input: observation.capture_content(input),
        metadata: Vec::new(),
    })
}

pub fn finish_retrieval(observation: Observation, result: &Result<RetrievalOutput, PipelineError>) {
    let outcome = match result {
        Ok(output) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(output),
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
        PipelineError::InconsistentData { code, .. } => (*code, "inconsistent_data"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Llm { .. }
        | PipelineError::Prompt { .. }
        | PipelineError::Narrative { .. }
        | PipelineError::EmptyOutput { .. }
        | PipelineError::InvalidOutput { .. } => ("pipeline_error", "pipeline"),
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
    };
    ObservationError {
        code: code.into(),
        failure_kind: failure_kind.into(),
        stage: Some("retrieval".into()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/retrieval_trace_tests.rs"]
mod tests;
