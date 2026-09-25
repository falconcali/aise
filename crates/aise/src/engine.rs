use crate::config::AiseConfig;
use crate::domain::ids::{TurnKey, TurnNumber};
use crate::observability::{
    Attribute, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus, Trace,
};
use crate::persistence::store::{Store, StoredTurnOutcome};
use crate::runtime::story_turn_coordinator::StoryTurnCoordinator;
use crate::runtime::turn_runtime::TurnRuntime;
use crate::turn::turn_budget::TurnBudget;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::{CommittedTurnResult, ExecuteTurnSpec, TurnControl, TurnIdentity};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind, TurnTerminalKind};
use crate::turn::turn_event::{TurnEvent, TurnEventSink};
#[cfg(test)]
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }
}

pub enum TurnRunOutcome {
    Committed {
        result: CommittedTurnResult,
        replayed: bool,
    },
    Failed(TurnExecutionError),
}

pub struct AiseEngine {
    runtime: TurnRuntime,
    store: Arc<dyn Store>,
    coordinator: Arc<StoryTurnCoordinator>,
    config: AiseConfig,
    clock: Arc<dyn Clock>,
}

impl AiseEngine {
    pub fn new(
        runtime: TurnRuntime,
        store: Arc<dyn Store>,
        coordinator: Arc<StoryTurnCoordinator>,
        config: AiseConfig,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            runtime,
            store,
            coordinator,
            config,
            clock,
        }
    }

    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    pub fn coordinator(&self) -> &Arc<StoryTurnCoordinator> {
        &self.coordinator
    }

    pub fn config(&self) -> &AiseConfig {
        &self.config
    }

    pub async fn run_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: Trace,
    ) -> Result<CommittedTurnResult, TurnExecutionError> {
        match self.execute_turn(spec, sink, trace).await {
            TurnRunOutcome::Committed { result, .. } => Ok(result),
            TurnRunOutcome::Failed(error) => Err(error),
        }
    }

    pub async fn execute_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        mut trace: Trace,
    ) -> TurnRunOutcome {
        let outcome = self.execute_turn_inner(spec, sink, &mut trace).await;
        finish_trace(trace, &outcome);
        outcome
    }

    async fn execute_turn_inner(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: &mut Trace,
    ) -> TurnRunOutcome {
        let validated = match spec.try_into_validated() {
            Ok(validated) => validated,
            Err(error) => {
                let failure = TurnExecutionError::new(
                    TurnFailureKind::InvalidRequest,
                    "invalid_request",
                    None,
                    error.to_string(),
                );
                return self.finalize(None, Err(failure), sink, None).await;
            }
        };
        let request = validated.request().clone();
        let story_id = validated.story_id().clone();
        let idempotency_key = validated.idempotency_key().clone();
        let cancellation = validated.cancellation().clone();
        let deadline = Instant::now() + Duration::from_millis(self.config.turn.turn_timeout_ms);

        let coordinate_span = engine_observation(trace, "coordinate-story-turn");
        let permit_result = coordinate_span
            .trace(self.coordinator.acquire(&story_id, deadline, &cancellation))
            .await;
        let permit = match permit_result {
            Ok(permit) => Some(permit),
            Err(error) => {
                coordinate_span.finish(execution_finish(&error));
                return self.finalize(None, Err(error), sink, None).await;
            }
        };
        coordinate_span.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        });

        let load_span = engine_observation(trace, "load-story");
        let story_info_result = load_span.trace(self.store.get_story(&story_id)).await;
        let story_info = match story_info_result {
            Ok(Some(info)) => info,
            Ok(None) => {
                load_span.finish(ObservationOutcome {
                    status: ObservationStatus::Error,
                    error: Some(ObservationError {
                        code: "story_not_found".into(),
                        failure_kind: "story_not_found".into(),
                        stage: None,
                        message: "story not found".into(),
                    }),
                    ..ObservationOutcome::default()
                });
                let failure = TurnExecutionError::new(
                    TurnFailureKind::StoryNotFound,
                    "story_not_found",
                    None,
                    format!("story {} not found", story_id.as_str()),
                );
                return self.finalize(None, Err(failure), sink, permit).await;
            }
            Err(error) => {
                let failure = TurnExecutionError::from(error);
                load_span.finish(execution_finish(&failure));
                return self.finalize(None, Err(failure), sink, permit).await;
            }
        };
        load_span.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        });

        let idempotency_span = engine_observation(trace, "check-idempotency");
        let replay_result = idempotency_span
            .trace(self.store.find_committed_turn(&story_id, &idempotency_key))
            .await;
        let replay = match replay_result {
            Ok(outcome) => outcome,
            Err(error) => {
                let failure = TurnExecutionError::from(error);
                idempotency_span.finish(execution_finish(&failure));
                return self.finalize(None, Err(failure), sink, permit).await;
            }
        };
        idempotency_span.finish(ObservationOutcome {
            status: ObservationStatus::Ok,
            ..ObservationOutcome::default()
        });
        if let Some(StoredTurnOutcome { request_digest, result }) = replay {
            if request_digest == *request.request_digest() {
                let outcome = TurnRunOutcome::Committed { result, replayed: true };
                return self.finalize(None, Ok(outcome), sink, permit).await;
            }
            let failure = TurnExecutionError::idempotency_conflict(None);
            return self.finalize(None, Err(failure), sink, permit).await;
        }

        let candidate_turn_number = match story_info
            .last_committed_turn_number
            .checked_add(1)
            .ok_or(crate::domain::ids::TurnNumberError::Overflow)
            .and_then(TurnNumber::try_new)
        {
            Ok(turn_number) => turn_number,
            Err(error) => {
                let failure = TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "turn_number_allocation_failed",
                    None,
                    error.to_string(),
                );
                return self.finalize(None, Err(failure), sink, permit).await;
            }
        };
        trace.bind(Attribute::u64("aise.trace.metadata.turn_number", candidate_turn_number.get()));

        let budget = match TurnBudget::from_config(
            &self.config.turn,
            &self.config.content,
            &self.config.retrieval,
            &self.config.state_extractor,
            &self.config.narrative,
            &self.config.activation,
        ) {
            Ok(budget) => budget,
            Err(error) => return self.finalize(None, Err(error), sink, permit).await,
        };
        let created_at = self.clock.now_millis();
        let identity = TurnIdentity::new(
            TurnKey::new(story_id.clone(), candidate_turn_number),
            idempotency_key,
            created_at,
        );
        let control = TurnControl::new(deadline, cancellation);
        let mut ctx = match TurnExecutionContext::new(identity, request, budget, control) {
            Ok(ctx) => ctx,
            Err(error) => return self.finalize(None, Err(error), sink, permit).await,
        };
        let runtime_outcome = self.runtime.run(&mut ctx, sink, trace).await;

        let result = match runtime_outcome {
            Ok(()) => match ctx.committed_result().cloned() {
                Some(result) => TurnRunOutcome::Committed {
                    result,
                    replayed: false,
                },
                None => {
                    TurnRunOutcome::Failed(TurnExecutionError::invariant("committed turn missing committed result"))
                }
            },
            Err(error) => TurnRunOutcome::Failed(error),
        };
        self.finalize(Some(ctx), Ok(result), sink, permit).await
    }

    async fn finalize(
        &self,
        mut ctx: Option<TurnExecutionContext>,
        result: Result<TurnRunOutcome, TurnExecutionError>,
        sink: &dyn TurnEventSink,
        _permit: Option<crate::runtime::story_turn_coordinator::StoryPermit>,
    ) -> TurnRunOutcome {
        match result {
            Ok(TurnRunOutcome::Committed { result, replayed }) => {
                let event = TurnEvent::Committed {
                    result: result.clone(),
                    replayed,
                };
                if sink.emit(event).is_err() {
                    tracing::warn!(
                        story_id = %result.story_revision,
                        error_kind = "terminal_delivery_failed",
                        "terminal committed event delivery failed"
                    );
                }
                TurnRunOutcome::Committed { result, replayed }
            }
            Ok(TurnRunOutcome::Failed(error)) | Err(error) => {
                let failure = self.normalize_error(error);
                if let Some(context) = ctx.as_mut() {
                    let terminal = match failure.terminal_kind() {
                        TurnTerminalKind::Failed => context.mark_failed(&failure),
                        TurnTerminalKind::Cancelled => context.mark_cancelled(&failure),
                        TurnTerminalKind::Conflict => context.mark_conflict(&failure),
                    };
                    if terminal.is_err() {
                        tracing::warn!(error_kind = "terminal_kind_conflict", "context terminal transition rejected");
                    }
                }
                self.emit_terminal(ctx.as_mut(), &failure, sink);
                TurnRunOutcome::Failed(failure)
            }
        }
    }

    fn emit_terminal(
        &self,
        ctx: Option<&mut TurnExecutionContext>,
        failure: &TurnExecutionError,
        sink: &dyn TurnEventSink,
    ) {
        let turn_number = ctx.map(|context| context.turn_number());
        let terminal_event = match failure.terminal_kind() {
            TurnTerminalKind::Failed => TurnEvent::Failed {
                turn_number,
                code: failure.code(),
            },
            TurnTerminalKind::Cancelled => TurnEvent::Cancelled {
                turn_number,
                code: failure.code(),
            },
            TurnTerminalKind::Conflict => TurnEvent::Conflict {
                turn_number,
                code: failure.code(),
            },
        };
        if sink.emit(terminal_event).is_err() {
            tracing::warn!(error_kind = "terminal_delivery_failed", "terminal event delivery failed");
        }
    }

    fn normalize_error(&self, error: TurnExecutionError) -> TurnExecutionError {
        error
    }
}

fn execution_finish(error: &TurnExecutionError) -> ObservationOutcome {
    ObservationOutcome {
        status: execution_status(error),
        error: Some(ObservationError {
            code: error.code().into(),
            failure_kind: format!("{:?}", error.kind()).to_lowercase(),
            stage: error.stage().map(|stage| stage.as_str().into()),
            message: error.to_string(),
        }),
        ..ObservationOutcome::default()
    }
}

fn execution_status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}

fn terminal_status(error: &TurnExecutionError) -> &'static str {
    match error.kind() {
        TurnFailureKind::Cancelled => "cancelled",
        TurnFailureKind::DeadlineExceeded => "deadline_exceeded",
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => "conflict",
        _ => "failed",
    }
}

fn finish_trace(mut trace: Trace, outcome: &TurnRunOutcome) {
    match outcome {
        TurnRunOutcome::Committed { result, replayed } => {
            let metadata = vec![
                Attribute::bool("aise.observation.metadata.replayed", *replayed),
                Attribute::string(
                    "aise.observation.metadata.terminal_status",
                    if *replayed { "replayed" } else { "committed" },
                ),
            ];
            trace.bind(Attribute::u64("aise.trace.metadata.turn_number", result.turn_number.get()));
            trace.finish(ObservationOutcome {
                status: ObservationStatus::Ok,
                metadata,
                ..ObservationOutcome::default()
            });
        }
        TurnRunOutcome::Failed(error) => {
            trace.finish(ObservationOutcome {
                status: execution_status(error),
                metadata: failure_metadata(error),
                error: Some(ObservationError {
                    code: error.code().into(),
                    failure_kind: failure_kind(error.kind()).into(),
                    stage: error.stage().map(|stage| stage.as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationOutcome::default()
            });
        }
    }
}

#[cfg(test)]
#[derive(Serialize)]
struct RootTraceOutput<'a> {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    story_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'static str>,
}

#[cfg(test)]
fn root_trace_output(outcome: &TurnRunOutcome) -> RootTraceOutput<'_> {
    match outcome {
        TurnRunOutcome::Committed { result, .. } => RootTraceOutput {
            status: "committed",
            story_text: Some(result.story_text.as_str()),
            error_code: None,
            failure_kind: None,
            stage: None,
        },
        TurnRunOutcome::Failed(error) => RootTraceOutput {
            status: terminal_status(error),
            story_text: None,
            error_code: Some(error.code()),
            failure_kind: Some(failure_kind(error.kind())),
            stage: error.stage().map(|stage| stage.as_str()),
        },
    }
}

const fn failure_kind(kind: TurnFailureKind) -> &'static str {
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

fn failure_metadata(error: &TurnExecutionError) -> Vec<Attribute> {
    let mut metadata = vec![
        Attribute::bool("aise.observation.metadata.replayed", false),
        Attribute::string("aise.observation.metadata.terminal_status", terminal_status(error)),
    ];
    if let Some(stage) = error.stage() {
        metadata.push(Attribute::string("aise.observation.metadata.failure_stage", stage.as_str()));
    }
    metadata
}

fn engine_observation(trace: &Trace, name: &'static str) -> crate::observability::Observation {
    trace.begin_observation(ObservationSpec {
        name,
        kind: ObservationKind::Chain,
        input: None,
        metadata: Vec::new(),
    })
}

#[cfg(test)]
#[path = "tests/engine_tests.rs"]
mod tests;
