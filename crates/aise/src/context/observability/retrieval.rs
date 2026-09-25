use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct RetrieveContextObservation {
    observation: Observation,
}

pub fn begin_retrieve_context(parent: &Observation, ctx: &TurnExecutionContext) -> RetrieveContextObservation {
    RetrieveContextObservation {
        observation: parent.begin(ObservationSpec {
            name: "retrieve-context",
            kind: ObservationKind::Retriever,
            input: None,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl RetrieveContextObservation {
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
                error: Some(error_info(error)),
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

fn error_info(error: &TurnExecutionError) -> ObservationError {
    ObservationError {
        code: error.code().into(),
        failure_kind: format!("{:?}", error.kind()).to_lowercase(),
        stage: error.stage().map(|stage| stage.as_str().into()),
        message: error.to_string(),
    }
}
