use crate::pipeline::common::PipelineError;
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

pub fn finish_plan(observation: Observation, llm_result: &Result<String, PipelineError>) {
    let outcome = match llm_result {
        Ok(story_plan) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&story_plan),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "llm_error".into(),
                failure_kind: "llm".into(),
                stage: Some("plan".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
