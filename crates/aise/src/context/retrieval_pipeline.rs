use async_trait::async_trait;

use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;

/// Fills the context gaps requested by the WriterPlan (Architecture.md §9).
///
/// Pipeline shape: retriever(s) -> context merger -> `ctx.retrieved_ctx`.
#[derive(Default)]
pub struct ContextRetrievalPipeline;

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> &'static str {
        "context_retrieval"
    }

    async fn execute(&self, _ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        // Framework stub: honor ctx.plan.retrieval_requests.
        Ok(())
    }
}
