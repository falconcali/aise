use async_trait::async_trait;

use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Simulates the current cognition of key characters requested by the plan
/// (Architecture.md §10). Outputs `ctx.character_thoughts`.
#[derive(Default)]
pub struct CharacterThinkPipeline;

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> &'static str {
        "character_think"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: honor ctx.plan.character_requests.
        Ok(())
    }
}
