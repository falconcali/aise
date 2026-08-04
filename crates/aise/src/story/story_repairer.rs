use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::error::AiseError;
use crate::llm::gateway::LlmGateway;
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoryRepairer {
    gateway: Arc<LlmGateway>,
}

impl StoryRepairer {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }

    pub fn gateway(&self) -> &Arc<LlmGateway> {
        &self.gateway
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
