use crate::domain::narrative::StoryTurn;
use crate::persistence::store::{OutboxRecord, Store, TurnCommitSpec};
use crate::turn::observability::{
    METADATA_COMMIT_STATUS, METADATA_GRAPH_REVISION, METADATA_STORY_ID, METADATA_TURN_NUMBER, ObservationAttribute,
    ObservationError, ObservationFinish, ObservationSpan, ObservationStatus, ObservationStep,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{Instrument, info_span};

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

    fn observation_input(&self, ctx: &TurnExecutionContext) -> serde_json::Value {
        let change_set = ctx.change_set();
        let story_text = change_set.map(|value| value.story_text());
        serde_json::json!({
            "phase": format!("{:?}", ctx.phase()).to_lowercase(),
            "story_text_bytes": story_text.map_or(0, str::len),
            "story_text_sha256": story_text.map(|value| crate::turn::observability::sha256_hex(value.as_bytes())),
            "new_role_count": change_set.map_or(0, |value| value.new_roles().len()),
            "role_change_count": change_set.map_or(0, |value| value.role_changes().len()),
            "knowledge_mutation_count": change_set.map_or(0, |value| value.knowledge_mutations().len()),
            "narrative_event_count": change_set.map_or(0, |value| value.narrative_events().len())
        })
    }

    fn observation_output(&self, ctx: &TurnExecutionContext, succeeded: bool) -> serde_json::Value {
        let result = ctx.committed_result();
        serde_json::json!({
            "committed": succeeded,
            "turn_number": result.map(|value| value.turn_number.get()),
            "story_revision": result.map(|value| value.story_revision.get()),
            "llm_call_count": result.map_or(0, |value| value.llm_calls.len())
        })
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
        let prepared_activation = ctx.activation().ok_or_else(|| {
            TurnExecutionError::new(
                crate::turn::turn_error::TurnFailureKind::InvariantViolation,
                "missing_activation",
                Some(TurnStage::TurnCommitter),
                "committer requires prepared activation state",
            )
        })?;
        let activation_state_delta = prepared_activation.pending_timed_state.clone();
        let overlay_version_before = prepared_activation.index_snapshot.reference.overlay_version;
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
        let timed_upserts = activation_state_delta.upserts.len();
        let timed_deletes = activation_state_delta.deletes.len();
        let overlay_version_after = if change_set
            .knowledge_mutations()
            .iter()
            .any(knowledge_mutation_affects_activation)
        {
            overlay_version_before.saturating_add(1)
        } else {
            overlay_version_before
        };
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
                player_contribution: ctx.player_contribution().to_string(),
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
            activation_state_delta,
        };
        let persistence_observation = ObservationSpan::begin_captured(
            ObservationStep::PersistTurn,
            Vec::new(),
            ctx.observation_capture().clone(),
            &serde_json::json!({
                "operation": "commit_turn",
                "base_revision": snapshot.base_revision().get(),
                "expected_graph_revision": snapshot.graph_revision(),
                "outbox_event_count": commit.outbox.len(),
                "timed_upserts": timed_upserts,
                "timed_deletes": timed_deletes
            }),
        );
        let activation_span = info_span!(
            "knowledge.activation.commit",
            story_id = %story_id,
            turn_number = turn_number.get(),
            timed_upserts,
            timed_deletes,
            overlay_version_before,
            overlay_version_after,
            status = tracing::field::Empty,
            error_code = tracing::field::Empty,
        );
        let outcome = self.store.commit_turn(&commit).instrument(activation_span.clone()).await;
        let persistence_finish = match &outcome {
            Ok(result) => ObservationFinish {
                status: ObservationStatus::Ok,
                metadata: vec![
                    ObservationAttribute::string(METADATA_STORY_ID, story_id.as_str()),
                    ObservationAttribute::u64(METADATA_TURN_NUMBER, turn_number.get()),
                    ObservationAttribute::string(METADATA_COMMIT_STATUS, "committed"),
                    ObservationAttribute::u64(METADATA_GRAPH_REVISION, result.story_revision.get()),
                ],
                ..ObservationFinish::default()
            },
            Err(error) => ObservationFinish {
                status: if matches!(
                    error,
                    crate::persistence::store::StoreError::RevisionConflict
                        | crate::persistence::store::StoreError::IdempotencyConflict
                ) {
                    ObservationStatus::Conflict
                } else {
                    ObservationStatus::Error
                },
                metadata: vec![
                    ObservationAttribute::string(METADATA_STORY_ID, story_id.as_str()),
                    ObservationAttribute::u64(METADATA_TURN_NUMBER, turn_number.get()),
                    ObservationAttribute::string(METADATA_COMMIT_STATUS, "failed"),
                ],
                error: Some(ObservationError {
                    code: store_error_code(error).into(),
                    failure_kind: "store".into(),
                    stage: Some(TurnStage::TurnCommitter.as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationFinish::default()
            },
        };
        persistence_observation.finish_captured(
            persistence_finish,
            &serde_json::json!({
                "commit_status": if outcome.is_ok() { "committed" } else { "failed" },
                "story_revision": outcome.as_ref().ok().map(|result| result.story_revision.get())
            }),
        );
        match &outcome {
            Ok(_) => {
                activation_span.record("status", "ok");
            }
            Err(error) => {
                activation_span.record("status", "error");
                activation_span.record("error_code", store_error_code(error));
            }
        }
        let result = outcome?;
        ctx.set_committed_result(result)
    }
}

fn knowledge_mutation_affects_activation(mutation: &crate::turn::turn_validation::ValidatedKnowledgeMutation) -> bool {
    match &mutation.operation {
        crate::turn::turn_validation::ValidatedKnowledgeOperation::Add(entry)
        | crate::turn::turn_validation::ValidatedKnowledgeOperation::Update { value: entry, .. } => {
            entry.kind() != crate::domain::knowledge::KnowledgeKind::Memory
        }
        crate::turn::turn_validation::ValidatedKnowledgeOperation::Delete { target } => {
            matches!(target, crate::domain::turn::DeletableKnowledgeId::Rumor(_))
        }
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
