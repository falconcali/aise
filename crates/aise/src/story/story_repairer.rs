use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
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
    fn stage(&self) -> TurnStage {
        TurnStage::StoryRepairer
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        Ok(())
    }
}
