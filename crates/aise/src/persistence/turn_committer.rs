use crate::domain::narrative::StoryTurn;
use crate::persistence::store::{OutboxRecord, Store, TurnCommitSpec};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
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
                crate::turn::turn_error::TurnFailureKind::InvariantViolation,
                "commit_gate_rejected",
                Some(TurnStage::TurnCommitter),
                format!("committer requires ReadyToCommit phase, current {:?}", ctx.phase()),
            ));
        }
        let change_set = ctx
            .change_set()
            .ok_or_else(|| {
                TurnExecutionError::new(
                    crate::turn::turn_error::TurnFailureKind::InvariantViolation,
                    "missing_change_set",
                    Some(TurnStage::TurnCommitter),
                    "committer requires a validated change set",
                )
            })?
            .clone();
        let snapshot = ctx
            .snapshot()
            .ok_or_else(|| {
                TurnExecutionError::new(
                    crate::turn::turn_error::TurnFailureKind::InvariantViolation,
                    "missing_snapshot",
                    Some(TurnStage::TurnCommitter),
                    "committer requires a story snapshot",
                )
            })?
            .clone();
        let story_text = change_set.story_text().to_owned();
        let turn_number = ctx.turn_number();
        let story_id = ctx.story_id().clone();
        let created_at = ctx.identity().started_at_ms();
        let llm_calls = ctx.llm_calls().to_vec();
        let mut outbox = Vec::new();
        for (seq, event) in change_set.narrative_events().iter().enumerate() {
            outbox.push(OutboxRecord {
                id: format!("{story_id}:turn:{turn_number}:outbox:{seq}"),
                story_id: story_id.clone(),
                turn_number,
                event_type: format!("story_event.{}", event.kind.as_str()),
                payload: serde_json::to_value(event).map_err(|_| {
                    TurnExecutionError::new(
                        crate::turn::turn_error::TurnFailureKind::InvariantViolation,
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
                number: turn_number,
                sequence: snapshot.story_continuity().next_sequence().map_err(|_| {
                    TurnExecutionError::new(
                        crate::turn::turn_error::TurnFailureKind::InvariantViolation,
                        "story_sequence_overflow",
                        Some(TurnStage::TurnCommitter),
                        "failed to assign next story sequence",
                    )
                })?,
                player_input: ctx.player_input().to_string(),
                story_text,
                created_at,
            },
            base_revision: snapshot.base_revision(),
            expected_graph_revision: snapshot.graph_revision(),
            changes: change_set.clone(),
            idempotency_key: ctx.identity().idempotency_key().clone(),
            request_digest: ctx.request().request_digest().clone(),
            outbox,
            llm_calls,
        };
        let pending = ctx.trace().begin_span("story.commit", "story.commit");
        let started = Instant::now();
        let outcome = self.store.commit_turn(&commit).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let payload = match &outcome {
            Ok(result) => serde_json::json!({
                "story_id": story_id.as_str(),
                "turn_number": turn_number.get(),
                "base_revision": snapshot.base_revision().get(),
                "committed_revision": result.story_revision.get(),
                "knowledge_mutation_count": change_set.knowledge_mutations().len(),
                "transition_count": change_set.narrative_resolution().transitions.len(),
                "status": "ok",
                "error_code": null,
                "latency_ms": latency_ms,
            }),
            Err(error) => serde_json::json!({
                "story_id": story_id.as_str(),
                "turn_number": turn_number.get(),
                "base_revision": snapshot.base_revision().get(),
                "committed_revision": null,
                "knowledge_mutation_count": change_set.knowledge_mutations().len(),
                "transition_count": change_set.narrative_resolution().transitions.len(),
                "status": "error",
                "error_code": store_error_code(error),
                "latency_ms": latency_ms,
            }),
        };
        ctx.trace().end_span_with(pending, &payload);
        let result = outcome?;
        ctx.set_committed_result(result)
    }
}

fn store_error_code(error: &crate::persistence::store::StoreError) -> &'static str {
    match error {
        crate::persistence::store::StoreError::NotFound => "story_not_found",
        crate::persistence::store::StoreError::RevisionConflict => "revision_conflict",
        crate::persistence::store::StoreError::IdempotencyConflict => "idempotency_conflict",
        crate::persistence::store::StoreError::ConstraintViolation { .. } => "constraint_violation",
        crate::persistence::store::StoreError::LimitExceeded { .. } => "store_limit_exceeded",
        crate::persistence::store::StoreError::Serialization { .. } => "store_serialization_error",
        crate::persistence::store::StoreError::Unavailable => "store_unavailable",
    }
}
