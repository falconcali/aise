use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{BaselineContext, SnapshotLimits, StoryConfig};
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
        let limits = SnapshotLimits {
            max_recent_turns: ctx.budget().max_retrieved_items(),
            max_memories: ctx.budget().max_retrieved_items(),
        };
        let snapshot = {
            let pending = ctx.trace().begin_span("aise.tool_call", "store.load_story_snapshot");
            let started = Instant::now();
            let outcome = self.store.load_story_snapshot(&story_id, limits).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(Some(snapshot)) => (true, serde_json::json!({ "revision": snapshot.base_revision().get() })),
                Ok(None) => (true, serde_json::json!({ "revision": null })),
                Err(error) => (false, serde_json::json!({ "error": error.to_string() })),
            };
            ctx.trace().end_span_with(
                pending,
                &SpanPayload::ToolCall(ToolCallData {
                    tool: "store.load_story_snapshot".into(),
                    args: serde_json::json!({ "story_id": story_id.to_string() }),
                    result,
                    ok,
                    latency_ms,
                }),
            );
            outcome?
        };
        let snapshot = snapshot.ok_or_else(|| AiseError::StoryNotFound(story_id.to_string()))?;
        let player_character = snapshot
            .player_character_id()
            .and_then(|player_id| snapshot.characters().iter().find(|c| c.id == *player_id).cloned());
        ctx.set_prepared_context(
            snapshot.clone(),
            BaselineContext {
                story_instructions: String::new(),
                story_config: StoryConfig::default(),
                player_character,
                current_scene: None,
                relevant_characters: snapshot.characters().to_vec(),
                recent_story: snapshot.recent_turns().iter().map(|t| t.story_text.clone()).collect(),
                story_summary: String::new(),
                active_constraints: Vec::new(),
            },
        )
    }
}
