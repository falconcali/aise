use async_trait::async_trait;

use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Assembles the AI's baseline cognition from persisted world state. Does not
/// generate story (Architecture.md §7).
#[derive(Default)]
pub struct BaselineContextBuilder;

#[async_trait]
impl TurnExecutionPipeline for BaselineContextBuilder {
    fn stage(&self) -> &'static str {
        "baseline_ctx_builder"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: load world + characters via Store and populate
        // ctx.baseline_ctx.
        Ok(())
    }
}
