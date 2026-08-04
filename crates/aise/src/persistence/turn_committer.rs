use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{CommittedTurnResult, TurnPhase};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{PersistData, SpanPayload};
use crate::domain::narrative::StoryTurn;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

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
    fn stage(&self) -> TurnStage {
        TurnStage::TurnCommitter
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        if ctx.phase() != TurnPhase::ReadyToCommit {
            return Err(AiseError::InvariantViolation(format!(
                "committer requires ReadyToCommit phase, current {:?}",
                ctx.phase()
            )));
        }
        let change_set = ctx
            .change_set()
            .ok_or_else(|| AiseError::InvariantViolation("committer requires a validated change set".into()))?;
        let story_text = change_set.story_text().to_owned();
        let events = change_set.events().to_vec();
        let characters = change_set.character_changes().to_vec();
        let world = change_set.world_change().clone();
        let memory = change_set.memory_changes().to_vec();
        let summary_delta = change_set.summary_delta().map(str::to_owned);
        let turn_id = ctx.turn_id().clone();
        let committed_story_text = story_text.clone();
        let commit = TurnCommit {
            story_id: ctx.story_id().clone(),
            turn: StoryTurn {
                id: turn_id.clone(),
                player_input: ctx.player_input().to_string(),
                story_text,
                summary_delta,
                created_at: ctx.identity().started_at_ms(),
            },
            events,
            characters,
            world,
            memory,
        };
        let pending = ctx.trace().begin_span("aise.persist", "turn_committer.commit");
        let started = Instant::now();
        let outcome = self.store.commit_turn(&commit).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let payload = match &outcome {
            Ok(()) => SpanPayload::Persist(PersistData {
                turn_id: turn_id.to_string(),
                status: "ok".into(),
                error: None,
                latency_ms,
            }),
            Err(error) => SpanPayload::Persist(PersistData {
                turn_id: turn_id.to_string(),
                status: "error".into(),
                error: Some(error.to_string()),
                latency_ms,
            }),
        };
        ctx.trace().end_span_with(pending, &payload);
        outcome?;
        ctx.set_committed_result(CommittedTurnResult {
            turn_id,
            story_text: committed_story_text,
        })
    }
}
