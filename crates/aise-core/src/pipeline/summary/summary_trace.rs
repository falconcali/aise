use crate::core::TurnNumber;
use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_summary(
    observation: &Observation,
    pending_turn_number: &TurnNumber,
    covered_turns: usize,
    covered_through: &TurnNumber,
) -> Observation {
    observation.begin(ObservationSpec {
        name: "story_summary",
        kind: ObservationKind::Span,
        input: observation.capture_content(&(pending_turn_number, covered_turns, covered_through)),
        metadata: Vec::new(),
    })
}

pub fn finish_summary(observation: Observation, result: &Result<String, PipelineError>) {
    let outcome = match result {
        Ok(summary_text) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(summary_text),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "llm_error".into(),
                failure_kind: "llm".into(),
                stage: Some("summary".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
