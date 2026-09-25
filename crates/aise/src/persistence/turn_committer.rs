use crate::domain::narrative::StoryTurn;
use crate::persistence::observability;
use crate::persistence::store::{OutboxRecord, Store, TurnCommitSpec};
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

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
        observation: &crate::observability::Observation,
    ) -> Result<(), TurnExecutionError> {
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
        let commit_observation = observability::begin_commit_turn(observation, ctx);
        let persistence_observation = observability::begin_persist_turn(observation, ctx);
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
        persistence_observation.finish(&outcome);
        match &outcome {
            Ok(_) => {
                activation_span.record("status", "ok");
            }
            Err(error) => {
                activation_span.record("status", "error");
                activation_span.record("error_code", observability::store_error_code(error));
            }
        }
        let result = outcome?;
        let committed = ctx.set_committed_result(result);
        commit_observation.finish(&committed);
        committed
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
