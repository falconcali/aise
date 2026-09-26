use crate::observability::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_repair_story(parent: &Observation) -> Observation {
    parent.begin(ObservationSpec {
        name: "repair-story",
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

pub fn begin_revise_story_text(parent: &Observation) -> Observation {
    parent.begin(ObservationSpec {
        name: "revise-story-text",
        kind: ObservationKind::Generation,
        input: None,
        metadata: Vec::new(),
    })
}

pub fn finish(observation: Observation, outcome: &Result<(), TurnExecutionError>) {
    observation.finish(outcome_for(outcome));
}

fn outcome_for(outcome: &Result<(), TurnExecutionError>) -> ObservationOutcome {
    match outcome {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: match error.kind() {
                TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
                TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
                TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
                _ => ObservationStatus::Error,
            },
            error: Some(ObservationError {
                code: error.code().into(),
                failure_kind: format!("{:?}", error.kind()).to_lowercase(),
                stage: error.stage().map(|stage| stage.as_str().into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    }
}
