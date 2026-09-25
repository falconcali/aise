use crate::observability::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct ValidateStoryObservation {
    observation: Observation,
}

pub fn begin_validate_story(parent: &Observation, ctx: &TurnExecutionContext) -> ValidateStoryObservation {
    ValidateStoryObservation {
        observation: parent.begin(ObservationSpec {
            name: "validate-story",
            kind: ObservationKind::Evaluator,
            input: None,
            metadata: vec![crate::observability::Attribute::string(
                "aise.observation.metadata.story_id",
                ctx.story_id().as_str(),
            )],
        }),
    }
}

impl ValidateStoryObservation {
    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(match outcome {
            Ok(()) => ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            },
            Err(error) => ObservationOutcome {
                status: match error.kind() {
                    TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
                    TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
                    TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => {
                        ObservationStatus::Conflict
                    }
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
        });
    }
}
