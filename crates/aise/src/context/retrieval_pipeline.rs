use crate::context::ctx_model::ContextItem;
use crate::error::AiseError;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{SpanPayload, ToolCallData};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::time::Instant;

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
            ctx.trace.record_span(
                "aise.tool_call",
                "context.retrieval",
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "context.retrieval".into(),
                    args: serde_json::json!({ "need_retrieval": false }),
                    result: serde_json::json!({ "items": 0 }),
                    ok: true,
                    latency_ms: 0,
                }),
            );
            return Ok(());
        }
        let limit = ctx.budget.max_retrieved_items;
        let pending = ctx.trace.begin_span("aise.tool_call", "context.retrieval");
        let started = Instant::now();
        let items: Vec<ContextItem> = ctx
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
        let latency_ms = started.elapsed().as_millis() as u64;
        ctx.trace.end_span_with(
            pending,
            &SpanPayload::ToolCall(ToolCallData {
                tool: "context.retrieval".into(),
                args: serde_json::json!({ "need_retrieval": true, "limit": limit }),
                result: serde_json::json!({ "items": items.len() }),
                ok: true,
                latency_ms,
            }),
        );
        ctx.retrieved_ctx = items;
        Ok(())
    }
}
