use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::TurnPhase;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{PersistData, SpanPayload};
use crate::domain::narrative::StoryTurn;
use crate::persistence::store::{OutboxRecord, Store, TurnCommitSpec};
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

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        if ctx.phase() != TurnPhase::ReadyToCommit {
            return Err(TurnExecutionError::new(
                crate::core::turn_error::TurnFailureKind::InvariantViolation,
                "commit_gate_rejected",
                Some(TurnStage::TurnCommitter),
                format!("committer requires ReadyToCommit phase, current {:?}", ctx.phase()),
            ));
        }
        let change_set = ctx.change_set().ok_or_else(|| {
            TurnExecutionError::new(
                crate::core::turn_error::TurnFailureKind::InvariantViolation,
                "missing_change_set",
                Some(TurnStage::TurnCommitter),
                "committer requires a validated change set",
            )
        })?;
        let snapshot = ctx.snapshot().ok_or_else(|| {
            TurnExecutionError::new(
                crate::core::turn_error::TurnFailureKind::InvariantViolation,
                "missing_snapshot",
                Some(TurnStage::TurnCommitter),
                "committer requires a story snapshot",
            )
        })?;
        let story_text = change_set.story_text().to_owned();
        let events = change_set.events().to_vec();
        let character_changes = change_set.character_changes().to_vec();
        let world_change = change_set.world_change();
        let memory_changes = change_set.memory_changes().to_vec();
        let scene_change = change_set.scene_change();
        let constraint_change = change_set.constraint_change();
        let summary_change = change_set.summary_change();
        let turn_id = ctx.turn_id().clone();
        let story_id = ctx.story_id().clone();
        let created_at = ctx.identity().started_at_ms();
        let llm_calls = ctx.llm_calls().to_vec();
        let mut outbox = Vec::new();
        for (seq, event) in change_set.events().iter().enumerate() {
            outbox.push(OutboxRecord {
                id: format!("{turn_id}#outbox#{seq}"),
                story_id: story_id.clone(),
                turn_id: turn_id.clone(),
                event_type: format!("story_event.{}", event.kind.as_str()),
                payload: serde_json::to_value(event).map_err(|_| {
                    TurnExecutionError::new(
                        crate::core::turn_error::TurnFailureKind::InvariantViolation,
                        "outbox_serialization_failed",
                        Some(TurnStage::TurnCommitter),
                        "failed to serialize outbox event payload",
                    )
                })?,
                created_at,
            });
        }
        let commit = TurnCommitSpec {
            story_id: story_id.clone(),
            turn: StoryTurn {
                id: turn_id.clone(),
                player_input: ctx.player_input().to_string(),
                story_text,
                created_at,
            },
            events,
            character_changes,
            world_change,
            memory_changes,
            scene_change,
            constraint_change,
            summary_change,
            base_revision: snapshot.base_revision(),
            idempotency_key: ctx.identity().idempotency_key().clone(),
            request_digest: ctx.request().request_digest().clone(),
            player_character_id: snapshot.player_character_id().cloned(),
            outbox,
            llm_calls,
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
