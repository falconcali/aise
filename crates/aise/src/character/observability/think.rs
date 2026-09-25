use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct ThinkCharacterObservation {
    observation: Observation,
}

pub fn begin_think_character(parent: &Observation, role_id: &str) -> ThinkCharacterObservation {
    ThinkCharacterObservation {
        observation: parent.begin(ObservationSpec {
            name: "think-character",
            kind: ObservationKind::Chain,
            input: None,
            metadata: vec![Attribute::string("aise.observation.metadata.character_id", role_id)],
        }),
    }
}

impl ThinkCharacterObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(match outcome {
            Ok(()) => ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            },
            Err(error) => ObservationOutcome {
                status: status(error),
                error: Some(ObservationError {
                    code: error.code().into(),
                    failure_kind: format!("{:?}", error.kind()).to_lowercase(),
                    stage: error.stage().map(|stage| stage.as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationOutcome::default()
            },
        });
    }
}

fn status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}
