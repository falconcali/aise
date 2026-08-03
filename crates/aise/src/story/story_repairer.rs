use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoryRepairer {
    #[allow(dead_code)]
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
        Ok(())
    }
}
