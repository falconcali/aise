use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{ContextItem, ContextSource};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use crate::error::AiseError;
use async_trait::async_trait;
use std::time::Instant;

#[derive(Default)]
pub struct ContextRetrievalPipeline;

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::ContextRetrieval
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let plan = ctx
            .plan()
            .ok_or_else(|| AiseError::InvariantViolation("writer plan not set before retrieval".into()))?
            .clone();
        if plan.retrieval_requests.is_empty() {
            let pending = ctx.trace().begin_span("aise.tool_call", "context.retrieval");
            ctx.trace().end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "context.retrieval".into(),
                    args: serde_json::json!({ "need_retrieval": false }),
                    result: serde_json::json!({ "items": 0 }),
                    ok: true,
                    latency_ms: 0,
                }),
            );
            return ctx.set_retrieved_context(Vec::new());
        }
        let limit = ctx.budget().max_retrieved_items();
        let items: Vec<ContextItem> = ctx
            .baseline()
            .ok_or_else(|| AiseError::InvariantViolation("baseline context not set before retrieval".into()))?
            .recent_story
            .iter()
            .take(limit)
            .map(|text| ContextItem {
                source: ContextSource::HistoricalStory,
                content: text.clone(),
                score: 1.0,
            })
            .collect();
        let pending = ctx.trace().begin_span("aise.tool_call", "context.retrieval");
        let started = Instant::now();
        let latency_ms = started.elapsed().as_millis() as u64;
        ctx.trace().end_span_with(
            pending,
            &SpanPayload::ToolCall(ToolCallData {
                tool: "context.retrieval".into(),
                args: serde_json::json!({ "need_retrieval": true, "limit": limit }),
                result: serde_json::json!({ "items": items.len() }),
                ok: true,
                latency_ms,
            }),
        );
        ctx.set_retrieved_context(items)
    }
}
