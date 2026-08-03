use crate::config::AiseConfig;
use crate::domain::ids::StoryId;
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::persistence::store::Store;
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
        self.runtime.run(&mut ctx, sink).await?;

        let turn_id = ctx.turn_id.clone();
        let story_text = ctx.draft.as_ref().map(|d| d.story_text.clone()).unwrap_or_default();

        sink.emit(TurnEvent::Validation {
            pass: ctx.validation.pass,
        });
        sink.emit(TurnEvent::Token(story_text.clone()));
        sink.emit(TurnEvent::Finished {
            turn_id: turn_id.clone(),
        });
        Ok(TurnResult { turn_id, story_text })
    }
}
