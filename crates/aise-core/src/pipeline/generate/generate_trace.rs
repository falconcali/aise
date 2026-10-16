use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_story_generation(observation: &Observation, story_goal: &str, player_contribution: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "story_generation",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(story_goal, player_contribution)),
        metadata: Vec::new(),
    })
}

pub fn finish_story_generation(observation: Observation, result: &Result<String, PipelineError>) {
    let outcome = match result {
        Ok(story_segment) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(story_segment),
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
        stage: Some("generate".to_owned()),
        message: error.to_string(),
    }
}
