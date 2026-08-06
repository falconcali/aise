use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;

#[derive(Default)]
pub struct TurnInitializer;

#[async_trait]
impl TurnExecutionPipeline for TurnInitializer {
    fn stage(&self) -> TurnStage {
        TurnStage::TurnInitializer
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        if ctx.player_input().trim().is_empty() {
            return Err(TurnExecutionError::invalid_request("empty player input"));
        }
        ctx.complete_initialization()
    }
}
