use crate::context::ctx_model::{BaselineContext, StoryConfig};
use crate::error::AiseError;
use crate::persistence::store::Store;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{SpanPayload, ToolCallData};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

pub struct BaselineContextBuilder {
    store: Arc<dyn Store>,
}

impl BaselineContextBuilder {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnExecutionPipeline for BaselineContextBuilder {
    fn stage(&self) -> &'static str {
        "baseline_ctx_builder"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let characters = {
            let pending = ctx.trace.begin_span("aise.tool_call", "store.load_characters");
            let started = Instant::now();
            let outcome = self.store.load_characters(&ctx.story_id).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(items) => (true, serde_json::json!({ "count": items.len() })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace.end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_characters".into(),
                    args: serde_json::json!({ "story_id": ctx.story_id.to_string() }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome?
        };
        let recent_story = {
            let pending = ctx.trace.begin_span("aise.tool_call", "store.load_story");
            let started = Instant::now();
            let outcome = self.store.load_story(&ctx.story_id, ctx.budget.max_retrieved_items).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(turns) => (true, serde_json::json!({ "count": turns.len() })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace.end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_story".into(),
                    args: serde_json::json!({
                        "story_id": ctx.story_id.to_string(),
                        "limit": ctx.budget.max_retrieved_items,
                    }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome?
        };
        ctx.baseline_ctx = BaselineContext {
            story_instructions: String::new(),
            story_config: StoryConfig::default(),
            player_character: characters.first().cloned(),
            current_scene: None,
            relevant_characters: characters,
            recent_story: recent_story.iter().map(|t| t.story_text.clone()).collect(),
            story_summary: String::new(),
            active_constraints: Vec::new(),
        };
        Ok(())
    }
}
