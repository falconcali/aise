use crate::config::AiseConfig;
use crate::core::turn_budget::TurnBudget;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{CommittedTurnResult, ExecuteTurnSpec, TurnControl, TurnIdentity, TurnRequest};
use crate::core::turn_data::SnapshotLimits;
use crate::core::turn_event::{TurnEvent, TurnEventSink};
use crate::core::turn_trace::{MAX_LLM_CONTENT_CHARS, SpanPayload, TraceRecorder, TraceSpanSink, TurnData, truncate};
use crate::domain::ids::TurnId;
use crate::error::AiseError;
use crate::persistence::store::Store;
use crate::runtime::story_turn_coordinator::StoryTurnCoordinator;
use crate::runtime::turn_runtime::TurnRuntime;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub trait IdGenerator: Send + Sync {
    fn new_turn_id(&self) -> TurnId;
}

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> i64;
}

pub struct UuidIdGenerator;

impl IdGenerator for UuidIdGenerator {
    fn new_turn_id(&self) -> TurnId {
        TurnId::from(Uuid::new_v4().to_string())
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
    ) -> Result<CommittedTurnResult, AiseError> {
        let request = TurnRequest::try_new(spec.player_input)?;
        let deadline = Instant::now() + Duration::from_millis(self.config.turn.turn_timeout_ms);
        let _permit = self.coordinator.acquire(&spec.story_id, deadline, &spec.cancellation).await?;
        if let Some(outcome) = self.store.find_committed_turn(&spec.story_id, &spec.idempotency_key).await? {
            if outcome.request_digest == *request.request_digest() {
                return Ok(outcome.result);
            }
            return Err(AiseError::IdempotencyConflict);
        }
        let budget = TurnBudget::from_config(&self.config.turn)?;
        let limits = SnapshotLimits {
            max_recent_turns: budget.max_retrieved_items(),
            max_memories: budget.max_retrieved_items(),
        };
        let created_at = self.clock.now_millis();
        if self.store.load_story_snapshot(&spec.story_id, limits).await?.is_none() {
            self.store.create_story(&spec.story_id, None, created_at).await?;
        }
        let identity = TurnIdentity::new(
            spec.story_id.clone(),
            self.id_generator.new_turn_id(),
            spec.idempotency_key,
            created_at,
        )?;
        let control = TurnControl::new(deadline, spec.cancellation);
        let mut recorder = TraceRecorder::with_limits(budget.max_trace_spans());
        if let Some(sink) = &self.trace_sink {
            recorder = recorder.with_sink(sink.clone());
        }
        let mut ctx = TurnExecutionContext::new(identity, request, budget, control, recorder)?;

        let root = ctx.trace().begin_span("aise.turn", "aise.turn");
        let outcome = self.runtime.run(&mut ctx, sink).await;

        let turn_id = ctx.turn_id().clone();
        let story_id_owned = ctx.story_id().clone();
        let player_input = truncate(ctx.player_input(), MAX_LLM_CONTENT_CHARS);
        let (status, error) = match &outcome {
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
        let trace = ctx.trace().build(&story_id_owned, &turn_id);
        if let Some(sink) = &self.trace_sink {
            sink.write_trace(&trace);
        }

        match &outcome {
            Ok(()) => {
                let committed = ctx
                    .committed_result()
                    .ok_or_else(|| AiseError::InvariantViolation("committed turn missing committed result".into()))?;
                sink.emit(TurnEvent::ValidationCompleted {
                    pass: ctx.validation().map(|v| v.is_pass()).unwrap_or(false),
                });
                sink.emit(TurnEvent::Committed(committed.clone()));
            }
            Err(error) => {
                let failed = ctx.turn_id().clone();
                let event = if matches!(error, AiseError::Cancelled) {
                    TurnEvent::Cancelled { turn_id: failed }
                } else if matches!(error, AiseError::IdempotencyConflict | AiseError::RevisionConflict) {
                    TurnEvent::Conflict { turn_id: failed }
                } else {
                    TurnEvent::Failed {
                        turn_id: failed,
                        error: error.to_string(),
                    }
                };
                sink.emit(event);
            }
        }
        sink.emit(TurnEvent::TraceCompleted(trace));
        outcome?;
        ctx.committed_result()
            .cloned()
            .ok_or_else(|| AiseError::InvariantViolation("committed turn missing committed result".into()))
    }
}
