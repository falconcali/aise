use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;

#[derive(Default)]
pub struct TurnInitializer;

#[async_trait]
impl TurnExecutionPipeline for TurnInitializer {
    fn stage(&self) -> TurnStage {
        TurnStage::TurnInitializer
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        if ctx.player_contribution().is_empty() {
            Err(TurnExecutionError::invalid_request("empty player contribution"))
        } else {
            ctx.complete_initialization()
        }
    }
}
