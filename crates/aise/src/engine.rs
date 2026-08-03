use crate::config::AiseConfig;
use crate::domain::ids::{StoryId, TurnId};
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::persistence::store::Store;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::runtime::turn_runtime::TurnRuntime;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted(&'static str),

    Token(String),

    Validation { pass: bool },

    Finished { turn_id: TurnId },
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}

#[derive(Debug, Clone)]
pub struct TurnResult {
    pub turn_id: TurnId,
    pub story_text: String,
}

pub struct AiseEngine {
    runtime: TurnRuntime,
    store: Arc<dyn Store>,
    llm: Arc<dyn LlmProvider>,
    config: AiseConfig,
}

impl AiseEngine {
    #[allow(clippy::too_many_arguments)]
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
        self.runtime.run(&mut ctx).await?;

        let turn_id = ctx.turn_id.clone();
        let story_text = ctx.draft.as_ref().map(|d| d.story_text.clone()).unwrap_or_default();

        sink.emit(TurnEvent::Finished {
            turn_id: turn_id.clone(),
        });
        Ok(TurnResult { turn_id, story_text })
    }
}
