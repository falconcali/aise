use crate::config::AiseConfig;
use crate::core::turn_budget::TurnBudget;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{CommittedTurnResult, ExecuteTurnSpec, TurnControl, TurnIdentity};
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind, TurnTerminalKind};
use crate::core::turn_event::{TurnEvent, TurnEventSink};
use crate::core::turn_trace::{MAX_LLM_CONTENT_CHARS, SpanPayload, TraceRecorder, TraceSpanSink, TurnData, truncate};
use crate::domain::ids::TurnId;
use crate::persistence::store::{Store, StoredTurnOutcome};
use crate::runtime::story_turn_coordinator::StoryTurnCoordinator;
use crate::runtime::turn_runtime::TurnRuntime;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub trait IdGenerator: Send + Sync {
    fn new_turn_id(&self) -> TurnId;
}

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> i64;
}

pub struct UuidIdGenerator;

impl IdGenerator for UuidIdGenerator {
    fn new_turn_id(&self) -> TurnId {
        TurnId::new_uuid()
    }
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
    id_generator: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    trace_sink: Option<Arc<dyn TraceSpanSink>>,
}

impl AiseEngine {
    pub fn new(
        runtime: TurnRuntime,
        store: Arc<dyn Store>,
        coordinator: Arc<StoryTurnCoordinator>,
        config: AiseConfig,
        id_generator: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            runtime,
            store,
            coordinator,
            config,
            id_generator,
            clock,
            trace_sink: None,
        }
    }

    pub fn with_trace_sink(mut self, trace_sink: Arc<dyn TraceSpanSink>) -> Self {
        self.trace_sink = Some(trace_sink);
        self
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
    ) -> Result<CommittedTurnResult, TurnExecutionError> {
        match self.execute_turn(spec, sink).await {
            TurnRunOutcome::Committed { result, .. } => Ok(result),
            TurnRunOutcome::Failed(error) => Err(error),
        }
    }

    pub async fn execute_turn(&self, spec: ExecuteTurnSpec, sink: &dyn TurnEventSink) -> TurnRunOutcome {
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

        let permit = match self.coordinator.acquire(&story_id, deadline, &cancellation).await {
            Ok(permit) => Some(permit),
            Err(error) => return self.finalize(None, Err(error), sink, None).await,
        };

        let story_exists = match self.store.get_story(&story_id).await {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(error) => {
                return self.finalize(None, Err(TurnExecutionError::from(error)), sink, permit).await;
            }
        };
        if !story_exists {
            let failure = TurnExecutionError::new(
                TurnFailureKind::StoryNotFound,
                "story_not_found",
                None,
                format!("story {} not found", story_id.as_str()),
            );
            return self.finalize(None, Err(failure), sink, permit).await;
        }

        let replay = match self.store.find_committed_turn(&story_id, &idempotency_key).await {
            Ok(outcome) => outcome,
            Err(error) => {
                return self.finalize(None, Err(TurnExecutionError::from(error)), sink, permit).await;
            }
        };
        if let Some(StoredTurnOutcome { request_digest, result }) = replay {
            if request_digest == *request.request_digest() {
                let outcome = TurnRunOutcome::Committed { result, replayed: true };
                return self.finalize(None, Ok(outcome), sink, permit).await;
            }
            let failure = TurnExecutionError::idempotency_conflict(None);
            return self.finalize(None, Err(failure), sink, permit).await;
        }

        let budget = match TurnBudget::from_config(&self.config.turn, &self.config.content) {
            Ok(budget) => budget,
            Err(error) => return self.finalize(None, Err(error), sink, permit).await,
        };
        let created_at = self.clock.now_millis();
        let identity =
            match TurnIdentity::new(story_id.clone(), self.id_generator.new_turn_id(), idempotency_key, created_at) {
                Ok(identity) => identity,
                Err(error) => {
                    let failure = TurnExecutionError::invalid_request(error.to_string());
                    return self.finalize(None, Err(failure), sink, permit).await;
                }
            };
        let control = TurnControl::new(deadline, cancellation);
        let mut recorder = TraceRecorder::with_limits(budget.max_trace_spans());
        if let Some(sink) = &self.trace_sink {
            recorder = recorder.with_sink(sink.clone());
        }
        let mut ctx = match TurnExecutionContext::new(identity, request, budget, control, recorder) {
            Ok(ctx) => ctx,
            Err(error) => return self.finalize(None, Err(error), sink, permit).await,
        };

        let root = ctx.trace().begin_span("aise.turn", "aise.turn");
        let runtime_outcome = self.runtime.run(&mut ctx, sink).await;

        let turn_id = ctx.turn_id().clone();
        let story_id_owned = ctx.story_id().clone();
        let player_input = truncate(ctx.player_input(), MAX_LLM_CONTENT_CHARS);
        let (status, error) = match &runtime_outcome {
            Ok(()) => ("ok", None),
            Err(e) => ("error", Some(e.to_string())),
        };
        ctx.trace().end_span_with(
            root,
            &SpanPayload::Turn(TurnData {
                story_id: story_id_owned.to_string(),
                turn_id: turn_id.to_string(),
                player_input,
                status: status.to_owned(),
                error,
            }),
        );

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
        let trace = ctx.as_mut().map(|context| {
            let turn_id = context.turn_id().clone();
            let story_id = context.story_id().clone();
            let trace = context.trace().build(&story_id, &turn_id);
            if let Some(sink) = &self.trace_sink {
                sink.write_trace(&trace);
            }
            (turn_id, Some(trace))
        });

        let outcome = match result {
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
        };
        if let Some((turn_id, Some(trace))) = trace {
            let _ = sink.emit(TurnEvent::TraceCompleted {
                turn_id,
                trace_id: trace.trace_id.clone(),
            });
        }
        outcome
    }

    fn emit_terminal(
        &self,
        ctx: Option<&mut TurnExecutionContext>,
        failure: &TurnExecutionError,
        sink: &dyn TurnEventSink,
    ) {
        let terminal_event = match failure.terminal_kind() {
            TurnTerminalKind::Failed => TurnEvent::Failed {
                turn_id: ctx.map(|context| context.turn_id().clone()).unwrap_or_else(TurnId::new_uuid),
                code: failure.code(),
            },
            TurnTerminalKind::Cancelled => TurnEvent::Cancelled {
                turn_id: ctx.map(|context| context.turn_id().clone()).unwrap_or_else(TurnId::new_uuid),
                code: failure.code(),
            },
            TurnTerminalKind::Conflict => TurnEvent::Conflict {
                turn_id: ctx.map(|context| context.turn_id().clone()).unwrap_or_else(TurnId::new_uuid),
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
