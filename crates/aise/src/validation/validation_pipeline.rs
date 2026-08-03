use async_trait::async_trait;

use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validators::consistency::ConsistencyValidator;
use crate::validation::validators::schema::SchemaValidator;

/// Runs deterministic + story validation and sets `ctx.validation`
/// (Architecture.md §13). The Repair/Validation loop budget is enforced by
/// `TurnRuntime` (R-AISE-06).
#[derive(Default)]
pub struct ValidationPipeline {
    schema: SchemaValidator,
    consistency: ConsistencyValidator,
}

#[async_trait]
impl TurnExecutionPipeline for ValidationPipeline {
    fn stage(&self) -> &'static str {
        "validation"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let mut result = self.schema.validate(ctx)?;
        if result.pass {
            result = self.consistency.validate(ctx).await?;
        }
        ctx.validation = result;
        Ok(())
    }
}
