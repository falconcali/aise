use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::persistence::store::StoreError;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub struct CommitTurnObservation {
    observation: Observation,
}

pub fn begin_commit_turn(parent: &Observation, ctx: &TurnExecutionContext) -> CommitTurnObservation {
    CommitTurnObservation {
        observation: parent.begin(ObservationSpec {
            name: "commit-turn",
            kind: ObservationKind::Chain,
            input: None,
            metadata: commit_metadata(ctx),
        }),
    }
}

impl CommitTurnObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, outcome: &Result<(), TurnExecutionError>) {
        self.observation.finish(turn_outcome(outcome));
    }
}

pub struct PersistTurnObservation {
    observation: Observation,
}

pub fn begin_persist_turn(parent: &Observation, ctx: &TurnExecutionContext) -> PersistTurnObservation {
    PersistTurnObservation {
        observation: parent.begin(ObservationSpec {
            name: "persist-turn",
            kind: ObservationKind::Tool,
            input: None,
            metadata: commit_metadata(ctx),
        }),
    }
}

impl PersistTurnObservation {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn finish<T>(self, outcome: &Result<T, StoreError>) {
        self.observation.finish(store_outcome(outcome));
    }
}

fn commit_metadata(ctx: &TurnExecutionContext) -> Vec<Attribute> {
    vec![
        Attribute::string("aise.observation.metadata.story_id", ctx.story_id().as_str()),
        Attribute::u64("aise.observation.metadata.turn_number", ctx.turn_number().get()),
    ]
}

fn store_outcome<T>(outcome: &Result<T, StoreError>) -> ObservationOutcome {
    match outcome {
        Ok(_) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata: vec![Attribute::string(
                "aise.observation.metadata.commit_status",
                "committed",
            )],
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: store_status(error),
            metadata: vec![Attribute::string("aise.observation.metadata.commit_status", "failed")],
            error: Some(ObservationError {
                code: store_error_code(error).into(),
                failure_kind: "store".into(),
                stage: Some("commit-turn".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    }
}

fn turn_outcome(outcome: &Result<(), TurnExecutionError>) -> ObservationOutcome {
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

fn store_status(error: &StoreError) -> ObservationStatus {
    match error {
        StoreError::RevisionConflict | StoreError::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}

pub(crate) fn store_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotFound => "story_not_found",
        StoreError::RevisionConflict => "revision_conflict",
        StoreError::IdempotencyConflict => "idempotency_conflict",
        StoreError::ConstraintViolation { .. } => "constraint_violation",
        StoreError::LimitExceeded { .. } => "store_limit_exceeded",
        StoreError::Serialization { .. } => "store_serialization_error",
        StoreError::Unavailable => "store_unavailable",
    }
}
