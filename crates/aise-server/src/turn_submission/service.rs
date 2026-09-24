use crate::observability::ObservabilityConfig;
use crate::session::{SessionId, SessionRegistry};
use crate::tasks::{TurnTaskSpec, TurnTaskSupervisor};
use aise::AiseEngine;
use aise::turn::observability::{
    BoundedContentEncoder, ContentCaptureLimits, ObservationAttribute, ObservationError, ObservationFields,
    ObservationFinish, ObservationSpan, ObservationStatus, ObservationStep, ObservationTrace, SESSION_ID,
    TRACE_ENVIRONMENT, TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST, TRACE_METADATA_STORY_ID, TRACE_RELEASE,
};
use aise::turn::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation, TurnRequest};
use aise::turn::turn_error::TurnFailureKind;
use aise::turn::turn_event::TurnEventSink;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::Instrument;

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
    trace_policy: aise::turn::observability::ContentCapturePolicy,
    trace_limits: ContentCaptureLimits,
    trace_environment: String,
    trace_release: String,
    trace_enabled: bool,
}

impl TurnSubmissionService {
    pub fn new(engine: Arc<AiseEngine>, registry: Arc<SessionRegistry>, tasks: Arc<TurnTaskSupervisor>) -> Self {
        let config = ObservabilityConfig::load_from_env().config;
        Self {
            engine,
            registry,
            tasks,
            trace_policy: config.content_policy,
            trace_limits: ContentCaptureLimits {
                max_field_bytes: config.max_field_bytes,
                max_observation_bytes: config.max_observation_bytes,
                detector_overlap_bytes: 512,
            },
            trace_environment: config.environment,
            trace_release: config.release,
            trace_enabled: config.enabled,
        }
    }

    pub async fn submit(
        &self,
        request: TurnSubmissionRequest,
        sink: Arc<dyn TurnEventSink>,
    ) -> Result<(), TurnSubmissionError> {
        let encoder = BoundedContentEncoder::new(self.trace_policy, self.trace_limits.clone());
        let mut trace = ObservationTrace::begin_with_content_capture(
            ObservationFields {
                metadata: vec![
                    ObservationAttribute::string(TRACE_ENVIRONMENT, self.trace_environment.clone()),
                    ObservationAttribute::string(TRACE_RELEASE, self.trace_release.clone()),
                ],
                input: self
                    .trace_enabled
                    .then(|| encoder.encode(&request.player_contribution, self.trace_limits.max_observation_bytes))
                    .flatten(),
            },
            self.trace_policy,
            self.trace_limits.clone(),
        );
        let root_span = trace.span();
        let prepared = async {
            let session_span = ObservationSpan::begin_with_parent(
                ObservationStep::ResolveInteractionSession,
                ObservationFields::default(),
                trace.context(),
            );
            let session_id = match SessionId::try_new(request.raw_session_id) {
                Ok(session_id) => session_id,
                Err(_) => {
                    let error = TurnSubmissionError::InvalidSession;
                    session_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            let session = match session_span.in_scope(self.registry.get(&session_id)).await {
                Some(session) => session,
                None => {
                    let error = TurnSubmissionError::SessionNotFound;
                    session_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            trace.bind_session(session.id.as_str(), session.story_id.as_str());
            session_span.finish(ObservationFinish {
                status: ObservationStatus::Ok,
                metadata: vec![
                    ObservationAttribute::string(SESSION_ID, session.id.as_str()),
                    ObservationAttribute::string(TRACE_METADATA_STORY_ID, session.story_id.as_str()),
                ],
                output: None,
                error: None,
                usage: None,
                cost: None,
            });

            let validation_span = ObservationSpan::begin_with_parent(
                ObservationStep::ValidateRequest,
                ObservationFields::default(),
                trace.context(),
            );
            if let Err(error) = TurnRequest::try_new(request.player_contribution.clone()) {
                let error = TurnSubmissionError::InvalidRequest(error.to_string());
                validation_span.finish(submission_finish(&error));
                return Err(error);
            }
            let raw_idempotency_key = match request.raw_idempotency_key {
                Some(key) => key,
                None => {
                    let error = TurnSubmissionError::MissingIdempotencyKey;
                    validation_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            let idempotency_key = match IdempotencyKey::try_new(raw_idempotency_key) {
                Ok(key) => key,
                Err(error) => {
                    let error = TurnSubmissionError::InvalidIdempotencyKey(error.to_string());
                    validation_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            let idempotency_key_digest = digest(idempotency_key.as_str());
            trace.bind_request(&idempotency_key_digest);
            validation_span.finish(ObservationFinish {
                status: ObservationStatus::Ok,
                metadata: vec![ObservationAttribute::string(
                    TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
                    idempotency_key_digest,
                )],
                output: None,
                error: None,
                usage: None,
                cost: None,
            });
            Ok(ExecuteTurnSpec {
                story_id: session.story_id.clone(),
                idempotency_key,
                player_contribution: request.player_contribution,
                cancellation: request.cancellation.clone(),
            })
        }
        .instrument(root_span.clone())
        .await;
        let spec = match prepared {
            Ok(spec) => spec,
            Err(error) => return finish_submission_error(trace, error),
        };
        let engine = self.engine.clone();
        let (trace_tx, trace_rx) = oneshot::channel::<ObservationTrace>();
        let task = TurnTaskSpec {
            cancellation: request.cancellation,
            future: Box::pin(async move {
                let Ok(trace) = trace_rx.await else {
                    return;
                };
                let span = trace.span();
                if let Err(error) = engine.run_turn(spec, sink.as_ref(), trace).instrument(span).await {
                    tracing::error!(
                        error = %error,
                        error_kind = failure_kind(error.kind()),
                        "turn task failed"
                    );
                }
            }),
        };
        let admission_parent = trace.context().clone();
        let admission = async {
            let admission_span = ObservationSpan::begin_with_parent(
                ObservationStep::AdmitTurnTask,
                ObservationFields::default(),
                &admission_parent,
            );
            let admission = admission_span.in_scope(self.tasks.spawn(task)).await;
            match &admission {
                Ok(()) => admission_span.finish(ObservationFinish {
                    status: ObservationStatus::Ok,
                    metadata: Vec::new(),
                    output: None,
                    error: None,
                    usage: None,
                    cost: None,
                }),
                Err(error) => {
                    let error = TurnSubmissionError::Admission(error.to_string());
                    admission_span.finish(submission_finish(&error));
                }
            }
            admission
        }
        .instrument(root_span)
        .await;
        if let Err(error) = admission {
            let error = TurnSubmissionError::Admission(error.to_string());
            return finish_submission_error(trace, error);
        }
        if let Err(trace) = trace_tx.send(trace) {
            return finish_submission_error(
                trace,
                TurnSubmissionError::Admission("admitted turn task stopped before trace transfer".into()),
            );
        }
        Ok(())
    }
}

fn finish_submission_error<T>(trace: ObservationTrace, error: TurnSubmissionError) -> Result<T, TurnSubmissionError> {
    trace.finish(submission_finish(&error));
    Err(error)
}

fn submission_finish(error: &TurnSubmissionError) -> ObservationFinish {
    ObservationFinish {
        status: ObservationStatus::Error,
        metadata: Vec::new(),
        output: None,
        error: Some(ObservationError {
            code: submission_error_code(error).into(),
            failure_kind: "submission".into(),
            stage: None,
            message: error.to_string(),
        }),
        usage: None,
        cost: None,
    }
}

fn submission_error_code(error: &TurnSubmissionError) -> &'static str {
    match error {
        TurnSubmissionError::InvalidSession => "invalid_session",
        TurnSubmissionError::SessionNotFound => "session_not_found",
        TurnSubmissionError::InvalidRequest(_) => "invalid_request",
        TurnSubmissionError::MissingIdempotencyKey => "missing_idempotency_key",
        TurnSubmissionError::InvalidIdempotencyKey(_) => "invalid_idempotency_key",
        TurnSubmissionError::Admission(_) => "turn_task_admission_failed",
    }
}

fn failure_kind(kind: TurnFailureKind) -> &'static str {
    match kind {
        TurnFailureKind::InvalidRequest => "invalid_request",
        TurnFailureKind::StoryNotFound => "story_not_found",
        TurnFailureKind::Cancelled => "cancelled",
        TurnFailureKind::DeadlineExceeded => "deadline_exceeded",
        TurnFailureKind::RevisionConflict => "revision_conflict",
        TurnFailureKind::IdempotencyConflict => "idempotency_conflict",
        TurnFailureKind::Backpressure => "backpressure",
        TurnFailureKind::ValidationRejected => "validation_rejected",
        TurnFailureKind::ValidationBudgetExhausted => "validation_budget_exhausted",
        TurnFailureKind::TokenBudgetExceeded => "token_budget_exceeded",
        TurnFailureKind::Llm => "llm",
        TurnFailureKind::Store => "store",
        TurnFailureKind::Io => "io",
        TurnFailureKind::InvariantViolation => "invariant_violation",
    }
}

fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "tests/service_tests.rs"]
mod tests;
