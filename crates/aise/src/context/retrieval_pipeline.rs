use crate::context::ctx_model::ContextItem;
use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;

#[derive(Default)]
pub struct ContextRetrievalPipeline;

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> &'static str {
        "context_retrieval"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let plan = ctx.plan.clone().unwrap_or_default();
        if !plan.need_retrieval {
            ctx.retrieved_ctx.clear();
            return Ok(());
        }
        let limit = ctx.budget.max_retrieved_items;
        let items = ctx
            .baseline_ctx
            .recent_story
            .iter()
            .take(limit)
            .map(|text| ContextItem {
                source: crate::context::ctx_model::ContextSource::HistoricalStory,
                content: text.clone(),
                score: 1.0,
            })
            .collect();
        ctx.retrieved_ctx = items;
        Ok(())
    }
}
