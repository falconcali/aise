use crate::observability::{Attribute, Observation, ObservationError, ObservationOutcome, ObservationStatus, Trace};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use std::future::Future;

pub struct RunTurnPipelinesObservation {
    observation: Observation,
}

impl RunTurnPipelinesObservation {
    pub fn begin(trace: &Trace) -> Self {
        Self {
            observation: trace.begin_observation(crate::observability::ObservationSpec {
                name: "run-turn-pipelines",
                kind: crate::observability::ObservationKind::Chain,
                input: None,
                metadata: Vec::new(),
            }),
        }
    }

    pub async fn trace<F: Future>(&self, future: F) -> F::Output {
        self.observation.trace(future).await
    }

    pub fn parent(&self) -> &Observation {
        &self.observation
    }

    pub fn finish(self, ctx: &TurnExecutionContext, result: &Result<(), TurnExecutionError>) {
        let metadata = vec![
            Attribute::bool("aise.observation.metadata.retrieval_skipped", ctx.retrieval_skipped()),
            Attribute::bool(
                "aise.observation.metadata.character_thinking_skipped",
                ctx.character_thinking_skipped(),
            ),
        ];
        self.observation.finish(match result {
            Ok(()) => ObservationOutcome {
                status: ObservationStatus::Ok,
                metadata,
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
