use crate::domain::ids::TurnId;
use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Default)]
pub struct TurnInitializer;

#[async_trait]
impl TurnExecutionPipeline for TurnInitializer {
    fn stage(&self) -> &'static str {
        "turn_initializer"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        if ctx.player_input.trim().is_empty() {
            return Err(AiseError::Internal("empty player input".into()));
        }
        ctx.turn_id = TurnId::from(Uuid::new_v4().to_string());
        Ok(())
    }
}
