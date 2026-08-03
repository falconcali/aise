use async_trait::async_trait;

use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// One step of a Story Turn.
///
/// Every pipeline mutates the shared `TurnExecutionContext` and never calls
/// other pipelines directly (R-AISE-01/R-AISE-02). Pipeline failures surface
/// as a typed error so the runtime can fail loudly (R-OBS-01) — an
/// engineering addition over the v1 trait sketch in Architecture.md §3.
#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    /// Stable stage name recorded in `ExecutionTrace`.
    fn stage(&self) -> &'static str;

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError>;
}
