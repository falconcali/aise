use crate::domain::narrative::StoryTurn;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{PersistData, SpanPayload};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct TurnCommitter {
    store: Arc<dyn Store>,
}

impl TurnCommitter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnExecutionPipeline for TurnCommitter {
    fn stage(&self) -> &'static str {
        "turn_committer"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let draft = ctx
            .draft
            .as_ref()
            .ok_or_else(|| AiseError::Internal("no draft to commit".into()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let commit = TurnCommit {
            story_id: ctx.story_id.clone(),
            turn: StoryTurn {
                id: ctx.turn_id.clone(),
                player_input: ctx.player_input.clone(),
                story_text: draft.story_text.clone(),
                summary_delta: None,
                created_at: now,
            },
            events: draft.events.clone(),
            characters: Vec::new(),
            world: None,
            memory: Vec::new(),
            summary: String::new(),
        };
        let pending = ctx.trace.begin_span("aise.persist", "turn_committer.commit");
        let started = Instant::now();
        let outcome = self.store.commit_turn(&commit).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let payload = match &outcome {
            Ok(()) => SpanPayload::Persist(PersistData {
                turn_id: ctx.turn_id.to_string(),
                status: "ok".into(),
                error: None,
                latency_ms,
            }),
            Err(error) => SpanPayload::Persist(PersistData {
                turn_id: ctx.turn_id.to_string(),
                status: "error".into(),
                error: Some(error.to_string()),
                latency_ms,
            }),
        };
        ctx.trace.end_span_with(pending, &payload);
        outcome
    }
}
