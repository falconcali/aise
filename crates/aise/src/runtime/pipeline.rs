use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;

#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> &'static str;

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError>;
}
