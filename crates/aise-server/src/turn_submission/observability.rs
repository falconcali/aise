use super::service::TurnSubmissionError;
use crate::observability::ObservabilityConfig;
use crate::session::Session;
use aise::observability::{
    Attribute, ContentCapture, ContentCapturePolicy, ObservabilityContentConfig, Observation, ObservationError,
    ObservationKind, ObservationOutcome, ObservationSession, ObservationSpec, ObservationStatus, SessionSpec, Trace,
    TraceSpec,
};
use aise_core::core::TurnResult;
use aise_core::engine::EngineError;
use sha2::{Digest, Sha256};

pub struct TurnSubmissionTrace {
    session: ObservationSession,
    trace: Trace,
    encoder: ContentCapture,
}

impl TurnSubmissionTrace {
    pub fn begin(player_contribution: &str) -> Self {
        let config = ObservabilityConfig::load_from_env().config;
        let encoder = ContentCapture::new(ContentCaptureConfig::from(&config).into_limits());
        let player_contribution = player_contribution.to_owned();
        let session = ObservationSession::begin(
            SessionSpec {
                id: None,
                user_id: None,
                metadata: Vec::new(),
            },
            encoder.clone(),
        );
        let trace = session.begin_trace(TraceSpec {
            name: "execute-story-turn",
            input: config
                .enabled
                .then(|| encoder.encode(&player_contribution, encoder.max_observation_bytes()).content)
                .flatten(),
            metadata: vec![
                Attribute::string("aise.trace.environment", config.environment),
                Attribute::string("aise.trace.release", config.release),
            ],
            tags: vec!["story-turn".to_owned()],
        });
        Self {
            session,
            trace,
            encoder,
        }
    }

    pub fn begin_session_resolution(&self) -> TurnSubmissionSpan {
        self.begin_observation(
            "resolve-interaction-session",
            ObservationKind::Retriever,
            serde_json::json!({"operation": "resolve-interaction-session"}),
        )
    }

    pub fn begin_request_validation(&self) -> TurnSubmissionSpan {
        self.begin_observation(
            "validate-request",
            ObservationKind::Chain,
            serde_json::json!({"operation": "validate-request"}),
        )
    }

    pub fn begin_task_admission(&self) -> TurnSubmissionSpan {
        self.begin_observation(
            "admit-turn-task",
            ObservationKind::Chain,
            serde_json::json!({"operation": "admit-turn-task"}),
        )
    }

    pub fn bind_session(&mut self, session: &Session) {
        self.trace.bind(vec![
            Attribute::string(aise::observability::SESSION_ID, session.id.as_str()),
            Attribute::string(aise::observability::TRACE_METADATA_STORY_ID, session.story_id.as_str()),
        ]);
    }

    pub fn bind_idempotency_key(&mut self, key: &str) {
        self.trace.bind(vec![Attribute::string(
            aise::observability::TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
            digest(key),
        )]);
    }

    pub fn finish_span(&self, span: TurnSubmissionSpan, result: Result<(), &TurnSubmissionError>) {
        let outcome = submission_outcome(&span.observation, result);
        span.finish(outcome);
    }

    pub fn finish_core_turn(self, result: &Result<TurnResult, EngineError>) {
        let outcome = match result {
            Ok(result) => ObservationOutcome {
                status: ObservationStatus::Ok,
                output: self
                    .encoder
                    .encode(&result.result.story_text, self.encoder.max_observation_bytes())
                    .content,
                ..ObservationOutcome::default()
            },
            Err(error) => ObservationOutcome {
                status: ObservationStatus::Error,
                error: Some(ObservationError {
                    code: "core_engine_error".into(),
                    failure_kind: "engine".into(),
                    stage: None,
                    message: error.to_string(),
                }),
                ..ObservationOutcome::default()
            },
        };
        self.trace.finish(outcome);
        self.session.finish(aise::observability::SessionOutcome {
            status: ObservationStatus::Ok,
            metadata: Vec::new(),
        });
    }

    pub fn finish_submission_error(self, error: TurnSubmissionError) -> TurnSubmissionError {
        let outcome = submission_outcome(self.trace.root(), Err(&error));
        self.trace.finish(outcome);
        self.session.finish(aise::observability::SessionOutcome {
            status: ObservationStatus::Ok,
            metadata: Vec::new(),
        });
        error
    }

    fn begin_observation(
        &self,
        name: &'static str,
        kind: ObservationKind,
        input: serde_json::Value,
    ) -> TurnSubmissionSpan {
        TurnSubmissionSpan {
            observation: self.trace.begin_observation(ObservationSpec {
                name,
                kind,
                input: self.trace.root().capture_content(&input),
                metadata: Vec::new(),
            }),
        }
    }
}

pub struct TurnSubmissionSpan {
    observation: Observation,
}

impl TurnSubmissionSpan {
    fn finish(self, outcome: ObservationOutcome) {
        self.observation.finish(outcome);
    }
}

fn submission_outcome(observation: &Observation, result: Result<(), &TurnSubmissionError>) -> ObservationOutcome {
    match result {
        Ok(()) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&serde_json::json!({"status": "completed"})),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: submission_error_code(error).into(),
                failure_kind: "submission".into(),
                stage: None,
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    }
}

pub(super) fn submission_error_code(error: &TurnSubmissionError) -> &'static str {
    match error {
        TurnSubmissionError::InvalidSession => "invalid_session",
        TurnSubmissionError::SessionNotFound => "session_not_found",
        TurnSubmissionError::InvalidRequest(_) => "invalid_request",
        TurnSubmissionError::MissingIdempotencyKey => "missing_idempotency_key",
        TurnSubmissionError::InvalidIdempotencyKey(_) => "invalid_idempotency_key",
        TurnSubmissionError::Admission(_) => "turn_task_admission_failed",
    }
}

pub(super) fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct ContentCaptureConfig {
    policy: ContentCapturePolicy,
    max_field_bytes: usize,
    max_observation_bytes: usize,
}

impl From<&ObservabilityConfig> for ContentCaptureConfig {
    fn from(config: &ObservabilityConfig) -> Self {
        Self {
            policy: config.content_policy.clone(),
            max_field_bytes: config.max_field_bytes,
            max_observation_bytes: config.max_observation_bytes,
        }
    }
}

impl ContentCaptureConfig {
    fn into_limits(self) -> ObservabilityContentConfig {
        ObservabilityContentConfig {
            policy: self.policy,
            max_field_bytes: self.max_field_bytes,
            max_observation_bytes: self.max_observation_bytes,
            detector_overlap_bytes: 512,
        }
    }
}
