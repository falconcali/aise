use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::error::AiseError;
use async_trait::async_trait;

#[derive(Default)]
pub struct TurnInitializer;

#[async_trait]
impl TurnExecutionPipeline for TurnInitializer {
    fn stage(&self) -> TurnStage {
        TurnStage::TurnInitializer
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        if ctx.player_input().trim().is_empty() {
            return Err(AiseError::InvalidRequest("empty player input".into()));
        }
        ctx.complete_initialization()
    }
}
