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

    fn observation_input(&self, ctx: &TurnExecutionContext) -> serde_json::Value {
        serde_json::json!({
            "phase": format!("{:?}", ctx.phase()).to_lowercase(),
            "player_contribution_bytes": ctx.player_contribution().len(),
            "player_contribution_sha256": crate::turn::observability::sha256_hex(ctx.player_contribution().as_bytes())
        })
    }

    fn observation_output(&self, ctx: &TurnExecutionContext, succeeded: bool) -> serde_json::Value {
        serde_json::json!({
            "completed": succeeded,
            "phase": format!("{:?}", ctx.phase()).to_lowercase()
        })
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        if ctx.player_contribution().is_empty() {
            Err(TurnExecutionError::invalid_request("empty player contribution"))
        } else {
            ctx.complete_initialization()
        }
    }
}
