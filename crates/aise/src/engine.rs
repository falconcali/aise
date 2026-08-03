use crate::config::AiseConfig;
use crate::domain::ids::StoryId;
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::persistence::store::Store;
use crate::runtime::trace::{MAX_LLM_CONTENT_CHARS, SpanPayload, TurnData, truncate};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::runtime::turn_runtime::TurnRuntime;
use std::sync::Arc;

pub use crate::runtime::event::{TurnEvent, TurnEventSink, TurnResult};

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
    ) -> Result<TurnResult, AiseError> {
        let mut ctx = TurnExecutionContext::new(story_id.clone(), player_input);
        let root = ctx.trace.begin_span("aise.turn", "aise.turn");
        let outcome = self.runtime.run(&mut ctx, sink).await;

        let turn_id = ctx.turn_id.clone();
        let story_text = ctx.draft.as_ref().map(|d| d.story_text.clone()).unwrap_or_default();
        let (status, error) = match &outcome {
            Ok(()) => ("ok", None),
            Err(e) => ("error", Some(e.to_string())),
        };
        ctx.trace.end_span_with(
            root,
            &SpanPayload::Turn(TurnData {
                story_id: ctx.story_id.to_string(),
                turn_id: turn_id.to_string(),
                player_input: truncate(&ctx.player_input, MAX_LLM_CONTENT_CHARS),
                status: status.to_owned(),
                error,
            }),
        );
        let trace = ctx.trace.build(&ctx.story_id, &turn_id);

        if outcome.is_ok() {
            sink.emit(TurnEvent::Validation {
                pass: ctx.validation.pass,
            });
            sink.emit(TurnEvent::Token(story_text.clone()));
            sink.emit(TurnEvent::Finished {
                turn_id: turn_id.clone(),
            });
        }
        sink.emit(TurnEvent::Trace(trace));
        outcome?;

        Ok(TurnResult { turn_id, story_text })
    }
}
