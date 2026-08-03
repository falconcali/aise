use std::sync::Arc;

use crate::config::AiseConfig;
use crate::domain::ids::{StoryId, TurnId};
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::persistence::store::Store;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::runtime::turn_runtime::TurnRuntime;

/// Events emitted during a Turn. The server layer forwards them over SSE.
#[derive(Debug, Clone)]
pub enum TurnEvent {
    /// A pipeline stage started (stable stage name from `stage()`).
    StageStarted(&'static str),
    /// Incremental text token from the generator.
    Token(String),
    /// Validation outcome after each round.
    Validation { pass: bool },
    /// The Turn committed successfully.
    Finished { turn_id: TurnId },
}

/// Receives Turn progress. Injected by the outer layer (R-LAYER-02); the
/// engine never knows the transport.
pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}

/// Result returned to the caller once a Turn commits.
#[derive(Debug, Clone)]
pub struct TurnResult {
    pub turn_id: TurnId,
    pub story_text: String,
}

/// Top-level engine entry point; the only object the server talks to.
///
/// Serialization of concurrent turns for the same story is the caller's
/// responsibility (the server's session registry owns that).
pub struct AiseEngine {
    runtime: TurnRuntime,
    store: Arc<dyn Store>,
    llm: Arc<dyn LlmProvider>,
    config: AiseConfig,
}

impl AiseEngine {
    #[allow(clippy::too_many_arguments)] // composition root wiring; bundling loses clarity
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

    /// Runs one full Turn for a story.
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
