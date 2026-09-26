use crate::observability::{Attribute, Observation, ObservationError, ObservationOutcome, ObservationStatus, Trace};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

pub fn begin_run_turn_pipelines(trace: &Trace, ctx: &TurnExecutionContext) -> Observation {
    trace.begin_observation(crate::observability::ObservationSpec {
        name: "run-turn-pipelines",
        kind: crate::observability::ObservationKind::Chain,
        input: trace.root().capture_content(&ctx.observation_input()),
        metadata: Vec::new(),
    })
}

pub fn finish(observation: Observation, ctx: &TurnExecutionContext, result: &Result<(), TurnExecutionError>) {
    let metadata = vec![
        Attribute::bool("aise.observation.metadata.retrieval_skipped", ctx.retrieval_skipped()),
        Attribute::bool(
            "aise.observation.metadata.character_thinking_skipped",
            ctx.character_thinking_skipped(),
        ),
    ];
    let output = result
        .as_ref()
        .ok()
        .and_then(|_| observation.capture_content(&ctx.observation_output()));
    observation.finish(match result {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            metadata,
            output,
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: status(error),
            metadata,
            error: Some(error_info(error)),
            ..ObservationOutcome::default()
        },
    });
}

fn error_info(error: &TurnExecutionError) -> ObservationError {
    ObservationError {
        code: error.code().into(),
        failure_kind: format!("{:?}", error.kind()).to_lowercase(),
        stage: error.stage().map(|stage| stage.as_str().into()),
        message: error.to_string(),
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
