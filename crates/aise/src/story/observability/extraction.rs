use crate::observability::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct ExtractStoryStateObservation {
    observation: Observation,
}

pub fn begin_extract_story_state(parent: &Observation) -> ExtractStoryStateObservation {
    ExtractStoryStateObservation {
        observation: parent.begin(ObservationSpec {
            name: "extract-story-state",
            kind: ObservationKind::Chain,
            input: None,
            metadata: Vec::new(),
        }),
    }
}

impl ExtractStoryStateObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(outcome_for(outcome));
    }
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
