use async_trait::async_trait;

use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use std::sync::Arc;

/// Fixes a Validation-rejected draft. Independent from `StoryGenerator`:
/// repair cares about constraint violations, logic errors, and consistency
/// (Architecture.md §12).
pub struct StoryRepairer {
    #[allow(dead_code)] // llm is exercised once repair prompt assembly is implemented
    llm: Arc<dyn LlmProvider>,
}

impl StoryRepairer {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryRepairer {
    fn stage(&self) -> &'static str {
        "story_repairer"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: repair ctx.draft against ctx.validation.issues.
        Ok(())
    }
}
