use crate::config::TurnContentLimitsConfig;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::{BaselineContext, SnapshotLimits};
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{SpanPayload, ToolCallData};
use crate::persistence::store::Store;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

pub struct BaselineContextBuilder {
    store: Arc<dyn Store>,
    content_limits: TurnContentLimitsConfig,
}

impl BaselineContextBuilder {
    pub fn new(store: Arc<dyn Store>, content_limits: TurnContentLimitsConfig) -> Self {
        Self { store, content_limits }
    }
}

#[async_trait]
impl TurnExecutionPipeline for BaselineContextBuilder {
    fn stage(&self) -> TurnStage {
        TurnStage::BaselineBuilder
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let story_id = ctx.story_id().clone();
        let limits = SnapshotLimits::from_config(&self.content_limits);
        let snapshot = {
            let pending = ctx.trace().begin_span("aise.tool_call", "store.load_story_snapshot");
            let started = Instant::now();
            let outcome = self.store.load_story_snapshot(&story_id, limits).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, result) = match &outcome {
                Ok(snapshot) => (true, serde_json::json!({ "revision": snapshot.base_revision().get() })),
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
        let player_character = snapshot
            .player_character_id()
            .and_then(|player_id| snapshot.characters().iter().find(|c| c.id == *player_id).cloned());
        let current_scene = if snapshot.current_scene().text.trim().is_empty() {
            None
        } else {
            Some(snapshot.current_scene().text.clone())
        };
        let story_summary = snapshot.story_summary().text.clone();
        ctx.set_prepared_context(
            snapshot.clone(),
            BaselineContext {
                story_instructions: snapshot.story_instructions().to_owned(),
                story_config: snapshot.story_config().clone(),
                player_character,
                current_scene,
                relevant_characters: snapshot.characters().to_vec(),
                recent_story: snapshot.recent_turns().iter().map(|t| t.story_text.clone()).collect(),
                story_summary,
                active_constraints: snapshot
                    .active_constraints()
                    .iter()
                    .map(|constraint| constraint.text.clone())
                    .collect(),
            },
        )
    }
}
