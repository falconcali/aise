use async_trait::async_trait;

use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use std::sync::Arc;

/// Creates the story from baseline context + plan + retrieved context +
/// character thoughts (Architecture.md §11). Outputs `ctx.draft`.
pub struct StoryGenerator {
    #[allow(dead_code)] // llm is exercised once prompt assembly is implemented
    llm: Arc<dyn LlmProvider>,
}

impl StoryGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> &'static str {
        "story_generator"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: build CompletionRequest, call self.llm.complete.
        Ok(())
    }
}
