use crate::core::PlayerContribution;
use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_player_contribution(observation: &Observation, player_input: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "player_contribution",
        kind: ObservationKind::Span,
        input: observation.capture_content(&player_input),
        metadata: Vec::new(),
    })
}

pub fn finish_player_contribution(observation: Observation, llm_result: &Result<PlayerContribution, PipelineError>) {
    let outcome = match llm_result {
        Ok(player_contribution) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&player_contribution),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "llm_error".into(),
                failure_kind: "llm".into(),
                stage: Some("baseline".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };

    observation.finish(outcome);
}
