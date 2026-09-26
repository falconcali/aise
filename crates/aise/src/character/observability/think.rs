use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_think_character(parent: &Observation, ctx: &TurnExecutionContext, role_id: &str) -> Observation {
    parent.begin(ObservationSpec {
        name: "think-character",
        kind: ObservationKind::Chain,
        input: parent.capture_content(&serde_json::json!({
            "turn": ctx.observation_input(),
            "role_id": role_id,
        })),
        metadata: vec![Attribute::string("aise.observation.metadata.character_id", role_id)],
    })
}

pub fn finish(observation: Observation, outcome: &Result<(), TurnExecutionError>) {
    let output = outcome.as_ref().ok().and_then(|_| {
        observation.capture_content(&serde_json::json!({
            "status": "character_decision_generated",
        }))
    });
    observation.finish(match outcome {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output,
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

fn status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}
