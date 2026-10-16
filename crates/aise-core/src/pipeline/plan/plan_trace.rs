use crate::pipeline::common::PipelineError;
use crate::pipeline::plan::PlanOutput;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_plan(observation: &Observation, player_input: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "plan",
        kind: ObservationKind::Span,
        input: observation.capture_content(&player_input),
        metadata: Vec::new(),
    })
}

pub fn finish_plan(observation: Observation, result: &Result<PlanOutput, PipelineError>) {
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
        PipelineError::InvalidOutput { code, .. } => (*code, "invalid_output"),
        PipelineError::Llm { .. } => ("llm_error", "llm"),
        PipelineError::Prompt { .. } => ("prompt_error", "prompt"),
        PipelineError::Persistence { .. } => ("persistence_error", "persistence"),
        PipelineError::Narrative { .. }
        | PipelineError::EmptyOutput { .. }
        | PipelineError::InconsistentData { .. } => ("pipeline_error", "pipeline"),
        PipelineError::InvalidConfig { code, .. } => (*code, "invalid_config"),
        PipelineError::ValidationFailed { code, .. } => (*code, "validation"),
    };
    ObservationError {
        code: code.into(),
        failure_kind: failure_kind.into(),
        stage: Some("plan".into()),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tests/plan_trace_tests.rs"]
mod tests;
