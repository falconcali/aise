use super::observability;
use crate::session::{SessionId, SessionRegistry};
use crate::tasks::{TurnTaskSpec, TurnTaskSupervisor};
use aise::AiseEngine;

use aise::turn::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation, TurnRequest};
use aise::turn::turn_event::TurnEventSink;
use std::sync::Arc;
use thiserror::Error;

pub struct TurnSubmissionRequest {
    pub raw_session_id: String,
    pub raw_idempotency_key: Option<String>,
    pub player_contribution: String,
    pub cancellation: TurnCancellation,
}

#[derive(Debug, Error)]
pub enum TurnSubmissionError {
    #[error("invalid session id")]
    InvalidSession,
    #[error("session not found")]
    SessionNotFound,
    #[error("invalid turn request: {0}")]
    InvalidRequest(String),
    #[error("missing Idempotency-Key header")]
    MissingIdempotencyKey,
    #[error("invalid idempotency key: {0}")]
    InvalidIdempotencyKey(String),
    #[error("turn task admission failed: {0}")]
    Admission(String),
}

pub struct TurnSubmissionService {
    engine: Arc<AiseEngine>,
    registry: Arc<SessionRegistry>,
    tasks: Arc<TurnTaskSupervisor>,
}

impl TurnSubmissionService {
    pub fn new(engine: Arc<AiseEngine>, registry: Arc<SessionRegistry>, tasks: Arc<TurnTaskSupervisor>) -> Self {
        Self {
            engine,
            registry,
            tasks,
        }
    }

    pub async fn submit(
        &self,
        request: TurnSubmissionRequest,
        sink: Arc<dyn TurnEventSink>,
    ) -> Result<(), TurnSubmissionError> {
        let mut trace = observability::TurnSubmissionTrace::begin(&request.player_contribution);
        let prepared = async {
            let session_span = trace.begin_session_resolution();
            let session_id = match SessionId::try_new(request.raw_session_id) {
                Ok(session_id) => session_id,
                Err(_) => {
                    let error = TurnSubmissionError::InvalidSession;
                    trace.finish_span(session_span, Err(&error));
                    return Err(error);
                }
            };
            let session = match self.registry.get(&session_id).await {
                Some(session) => session,
                None => {
                    let error = TurnSubmissionError::SessionNotFound;
                    trace.finish_span(session_span, Err(&error));
                    return Err(error);
                }
            };
            trace.bind_session(&session);
            trace.finish_span(session_span, Ok(()));
            let validation_span = trace.begin_request_validation();
            if let Err(error) = TurnRequest::try_new(request.player_contribution.clone()) {
                let error = TurnSubmissionError::InvalidRequest(error.to_string());
                trace.finish_span(validation_span, Err(&error));
                return Err(error);
            }
            let raw_idempotency_key = match request.raw_idempotency_key {
                Some(key) => key,
                None => {
                    let error = TurnSubmissionError::MissingIdempotencyKey;
                    trace.finish_span(validation_span, Err(&error));
                    return Err(error);
                }
            };
            let idempotency_key = match IdempotencyKey::try_new(raw_idempotency_key) {
                Ok(key) => key,
                Err(error) => {
                    let error = TurnSubmissionError::InvalidIdempotencyKey(error.to_string());
                    trace.finish_span(validation_span, Err(&error));
                    return Err(error);
                }
            };
            trace.bind_idempotency_key(idempotency_key.as_str());
            trace.finish_span(validation_span, Ok(()));
            Ok(ExecuteTurnSpec {
                story_id: session.story_id.clone(),
                idempotency_key,
                player_contribution: request.player_contribution,
                cancellation: request.cancellation.clone(),
            })
        }
        .await;
        let spec = match prepared {
            Ok(spec) => spec,
            Err(error) => return Err(trace.finish_submission_error(error)),
        };
        let engine = self.engine.clone();
        let admission = trace.begin_task_admission();
        let permit = match self.tasks.reserve(&request.cancellation).await {
            Ok(permit) => permit,
            Err(error) => {
                let error = TurnSubmissionError::Admission(error.to_string());
                trace.finish_span(admission, Err(&error));
                return Err(trace.finish_submission_error(error));
            }
        };
        trace.finish_span(admission, Ok(()));
        let task = TurnTaskSpec {
            cancellation: request.cancellation,
            future: Box::pin(async move {
                let result = engine.run_turn(spec, sink.as_ref(), trace.trace()).await;
                trace.finish_turn(&result);
                if let Err(error) = result {
                    tracing::error!(
                        error = %error,
                        error_kind = ?error.kind(),
                        "turn task failed"
                    );
                }
            }),
        };
        self.tasks.spawn_reserved(permit, task);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/service_tests.rs"]
mod tests;
