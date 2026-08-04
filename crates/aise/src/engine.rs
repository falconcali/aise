use crate::config::AiseConfig;
use crate::core::turn_budget::TurnBudget;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{
    CommittedTurnResult, IdempotencyKey, TurnCancellation, TurnControl, TurnIdentity, TurnRequest,
};
use crate::core::turn_event::{TurnEvent, TurnEventSink};
use crate::core::turn_trace::{MAX_LLM_CONTENT_CHARS, SpanPayload, TraceRecorder, TurnData, truncate};
use crate::domain::ids::{StoryId, TurnId};
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::persistence::store::Store;
use crate::runtime::turn_runtime::TurnRuntime;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub struct AiseEngine {
    runtime: TurnRuntime,
    store: Arc<dyn Store>,
    llm: Arc<dyn LlmProvider>,
    config: AiseConfig,
}

impl AiseEngine {
    pub fn new(runtime: TurnRuntime, store: Arc<dyn Store>, llm: Arc<dyn LlmProvider>, config: AiseConfig) -> Self {
        Self {
            runtime,
            store,
            llm,
            config,
        }
    }

    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    pub fn llm(&self) -> &Arc<dyn LlmProvider> {
        &self.llm
    }

    pub fn config(&self) -> &AiseConfig {
        &self.config
    }

    pub async fn run_turn(
        &self,
        story_id: &StoryId,
        player_input: String,
        sink: &dyn TurnEventSink,
    ) -> Result<CommittedTurnResult, AiseError> {
        let request = TurnRequest::try_new(player_input)?;
        let identity = TurnIdentity::new(
            story_id.clone(),
            TurnId::from(Uuid::new_v4().to_string()),
            IdempotencyKey::try_new(Uuid::new_v4().to_string())?,
            now_millis(),
        )?;
        let budget = TurnBudget::new(
            self.config.turn.max_repair_rounds,
            self.config.turn.max_tokens,
            self.config.turn.max_retrieved_items,
        );
        let control = TurnControl::new(
            Instant::now() + Duration::from_millis(self.config.turn.turn_timeout_ms),
            TurnCancellation::new(),
        );
        let mut ctx = TurnExecutionContext::new(identity, request, budget, control, TraceRecorder::new())?;

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

        if outcome.is_ok() {
            let committed = ctx
                .committed_result()
                .ok_or_else(|| AiseError::InvariantViolation("committed turn missing committed result".into()))?;
            sink.emit(TurnEvent::Validation {
                pass: ctx.validation().map(|v| v.pass).unwrap_or(false),
            });
            sink.emit(TurnEvent::Token(committed.story_text.clone()));
            sink.emit(TurnEvent::Finished {
                turn_id: committed.turn_id.clone(),
            });
        }
        sink.emit(TurnEvent::Trace(trace));
        outcome?;
        ctx.committed_result()
            .cloned()
            .ok_or_else(|| AiseError::InvariantViolation("committed turn missing committed result".into()))
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
