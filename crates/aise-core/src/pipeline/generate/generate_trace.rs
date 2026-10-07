use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_story_generation(observation: &Observation, story_goal: &str, player_contribution: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "story_generation",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(story_goal.to_owned(), player_contribution.to_owned())),
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
            error: Some(ObservationError {
                code: "llm_error".into(),
                failure_kind: "llm".into(),
                stage: Some("generate".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
