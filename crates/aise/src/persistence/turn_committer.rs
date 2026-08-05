use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::{LlmUsageAggregate, TurnPhase};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{PersistData, SpanPayload};
use crate::domain::narrative::StoryTurn;
use crate::error::AiseError;
use crate::persistence::store::{OutboxRecord, Store, TurnCommit};
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
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| AiseError::InvariantViolation("committer requires a story snapshot".into()))?;
        let story_text = change_set.story_text().to_owned();
        let events = change_set.events().to_vec();
        let characters = change_set.character_changes().to_vec();
        let world = change_set.world_change().clone();
        let memory = change_set.memory_changes().to_vec();
        let summary_delta = change_set.summary_delta().map(str::to_owned);
        let turn_id = ctx.turn_id().clone();
        let story_id = ctx.story_id().clone();
        let created_at = ctx.identity().started_at_ms();
        let budget = ctx.budget();
        let mut outbox = Vec::new();
        for (seq, event) in change_set.events().iter().enumerate() {
            outbox.push(OutboxRecord {
                id: format!("{turn_id}#outbox#{seq}"),
                story_id: story_id.clone(),
                turn_id: turn_id.clone(),
                event_type: format!("story_event.{}", event.kind.as_str()),
                payload: serde_json::to_value(event)?,
                created_at,
            });
        }
        let commit = TurnCommit {
            story_id: story_id.clone(),
            turn: StoryTurn {
                id: turn_id.clone(),
                player_input: ctx.player_input().to_string(),
                story_text,
                summary_delta,
                created_at,
            },
            events,
            characters,
            world,
            memory,
            base_revision: snapshot.base_revision(),
            idempotency_key: ctx.identity().idempotency_key().clone(),
            request_digest: ctx.request().request_digest().clone(),
            player_character_id: snapshot.player_character_id().cloned(),
            outbox,
            llm_usage: LlmUsageAggregate {
                llm_calls: budget.llm_calls(),
                input_tokens: budget.input_tokens(),
                output_tokens: budget.output_tokens(),
                total_tokens: budget.total_tokens(),
            },
        };
        let pending = ctx.trace().begin_span("aise.persist", "turn_committer.commit");
        let started = Instant::now();
        let outcome = self.store.commit_turn(&commit).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let payload = match &outcome {
            Ok(_) => SpanPayload::Persist(PersistData {
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
        let result = outcome?;
        ctx.set_committed_result(result)
    }
}
