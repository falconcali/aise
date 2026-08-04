use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{BaselineContext, StoryConfig};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use crate::error::AiseError;
use crate::persistence::store::Store;
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
    fn stage(&self) -> TurnStage {
        TurnStage::BaselineBuilder
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let story_id = ctx.story_id().clone();
        let limit = ctx.budget().max_retrieved_items();
        let characters = {
            let pending = ctx.trace().begin_span("aise.tool_call", "store.load_characters");
            let started = Instant::now();
            let outcome = self.store.load_characters(&story_id).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(items) => (true, serde_json::json!({ "count": items.len() })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace().end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_characters".into(),
                    args: serde_json::json!({ "story_id": story_id.to_string() }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome?
        };
        let recent_story = {
            let pending = ctx.trace().begin_span("aise.tool_call", "store.load_story");
            let started = Instant::now();
            let outcome = self.store.load_story(&story_id, limit).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(turns) => (true, serde_json::json!({ "count": turns.len() })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace().end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_story".into(),
                    args: serde_json::json!({
                        "story_id": story_id.to_string(),
                        "limit": limit,
                    }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome?
        };
        ctx.set_prepared_context(BaselineContext {
            story_instructions: String::new(),
            story_config: StoryConfig::default(),
            player_character: characters.first().cloned(),
            current_scene: None,
            relevant_characters: characters,
            recent_story: recent_story.iter().map(|t| t.story_text.clone()).collect(),
            story_summary: String::new(),
            active_constraints: Vec::new(),
        })
    }
}
