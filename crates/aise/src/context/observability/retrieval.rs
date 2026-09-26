use crate::observability::{
    Attribute, Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_retrieve_context(parent: &Observation, ctx: &TurnExecutionContext) -> Observation {
    parent.begin(ObservationSpec {
        name: "retrieve-context",
        kind: ObservationKind::Retriever,
        input: parent.capture_content(&ctx.observation_input()),
        metadata: vec![Attribute::string(
            "aise.observation.metadata.story_id",
            ctx.story_id().as_str(),
        )],
    })
}

pub fn finish(observation: Observation, outcome: &Result<(), TurnExecutionError>) {
    let output = outcome.as_ref().ok().and_then(|_| {
        observation.capture_content(&serde_json::json!({
            "status": "context_retrieved",
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
            error: Some(error_info(error)),
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

fn error_info(error: &TurnExecutionError) -> ObservationError {
    ObservationError {
        code: error.code().into(),
        failure_kind: format!("{:?}", error.kind()).to_lowercase(),
        stage: error.stage().map(|stage| stage.as_str().into()),
        message: error.to_string(),
    }
}
