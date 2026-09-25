use crate::observability::ObservabilityConfig;
use crate::session::{SessionId, SessionRegistry};
use crate::tasks::{TurnTaskSpec, TurnTaskSupervisor};
use aise::AiseEngine;
use aise::observability::{
    Attribute, ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, ObservationError, ObservationOutcome,
    ObservationSession, ObservationSpec, ObservationStatus, SessionSpec, Trace, TraceSpec,
};
use aise::turn::turn_contract::{ExecuteTurnSpec, IdempotencyKey, TurnCancellation, TurnRequest};
use aise::turn::turn_error::TurnFailureKind;
use aise::turn::turn_event::TurnEventSink;
use sha2::{Digest, Sha256};
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
    trace_limits: ObservabilityContentConfig,
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
            trace_limits: ObservabilityContentConfig {
                policy: match config.content_policy {
                    ContentCapturePolicy::MetadataOnly => ContentCapturePolicy::MetadataOnly,
                    ContentCapturePolicy::RedactedContent => ContentCapturePolicy::RedactedContent,
                    ContentCapturePolicy::FullContent => ContentCapturePolicy::FullContent,
                },
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
        let encoder = ContentCapture::new(self.trace_limits.clone());
        let session = ObservationSession::begin(
            SessionSpec {
                id: None,
                user_id: None,
                metadata: Vec::new(),
            },
            encoder.clone(),
        );
        let mut trace = session.begin_trace(TraceSpec {
            name: "execute-story-turn",
            input: self
                .trace_enabled
                .then(|| {
                    encoder
                        .encode(&request.player_contribution, self.trace_limits.max_observation_bytes)
                        .content
                })
                .flatten(),
            metadata: vec![
                Attribute::string("aise.trace.environment", self.trace_environment.clone()),
                Attribute::string("aise.trace.release", self.trace_release.clone()),
            ],
            tags: vec!["story-turn".to_owned()],
        });
        let prepared = async {
            let session_span = trace.begin_observation(ObservationSpec {
                name: "resolve-interaction-session",
                kind: aise::observability::ObservationKind::Retriever,
                input: None,
                metadata: Vec::new(),
            });
            let session_id = match SessionId::try_new(request.raw_session_id) {
                Ok(session_id) => session_id,
                Err(_) => {
                    let error = TurnSubmissionError::InvalidSession;
                    session_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            let session = match session_span.trace(self.registry.get(&session_id)).await {
                Some(session) => session,
                None => {
                    let error = TurnSubmissionError::SessionNotFound;
                    session_span.finish(submission_finish(&error));
                    return Err(error);
                }
            };
            trace.bind(vec![
                Attribute::string(aise::observability::SESSION_ID, session.id.as_str()),
                Attribute::string(aise::observability::TRACE_METADATA_STORY_ID, session.story_id.as_str()),
            ]);
            session_span.finish(ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            });

            let validation_span = trace.begin_observation(ObservationSpec {
                name: "validate-request",
                kind: aise::observability::ObservationKind::Chain,
                input: None,
                metadata: Vec::new(),
            });
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
            trace.bind(vec![Attribute::string(
                aise::observability::TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
                idempotency_key_digest,
            )]);
            validation_span.finish(ObservationOutcome {
                status: ObservationStatus::Ok,
                ..ObservationOutcome::default()
            });
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
            Err(error) => return finish_submission_error(trace, error),
        };
        let engine = self.engine.clone();
        let admission = trace.begin_observation(ObservationSpec {
            name: "admit-turn-task",
            kind: aise::observability::ObservationKind::Chain,
            input: None,
            metadata: Vec::new(),
        });
        let permit = match admission.trace(self.tasks.reserve(&request.cancellation)).await {
            Ok(permit) => permit,
            Err(error) => {
                let error = TurnSubmissionError::Admission(error.to_string());
                admission.finish(submission_finish(&error));
                return finish_submission_error(trace, error);
            }
        };
        admission.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        });
        let task = TurnTaskSpec {
            cancellation: request.cancellation,
            future: Box::pin(async move {
                if let Err(error) = engine.run_turn(spec, sink.as_ref(), trace).await {
                    tracing::error!(
                        error = %error,
                        error_kind = failure_kind(error.kind()),
                        "turn task failed"
                    );
                }
            }),
        };
        self.tasks.spawn_reserved(permit, task);
        session.finish(aise::observability::SessionOutcome {
            status: ObservationStatus::Ok,
            metadata: Vec::new(),
        });
        Ok(())
    }
}

fn finish_submission_error<T>(trace: Trace, error: TurnSubmissionError) -> Result<T, TurnSubmissionError> {
    trace.finish(submission_finish(&error));
    Err(error)
}

fn submission_finish(error: &TurnSubmissionError) -> ObservationOutcome {
    ObservationOutcome {
        status: aise::observability::ObservationStatus::Error,
        error: Some(ObservationError {
            code: submission_error_code(error).into(),
            failure_kind: "submission".into(),
            stage: None,
            message: error.to_string(),
        }),
        ..ObservationOutcome::default()
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
