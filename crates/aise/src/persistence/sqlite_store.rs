use crate::domain::ids::{RoleId, StoryId, StoryRevision};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::role::StoryRole;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::story_instance::state::{RelationshipKey, RelationshipState};
use crate::domain::turn::SnapshotLimits;
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_snapshot;
use crate::persistence::store::{OutboxRecord, Store, StoreError, StoredTurnOutcome, TurnCommitSpec};
use crate::turn::turn_contract::{CommittedTurnResult, IdempotencyKey};
use crate::turn::turn_validation::StateChange;
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(url: &str) -> Result<Arc<Self>, StoreError> {
        ensure_database_dir(url)?;
        let options = SqliteConnectOptions::from_str(url)
            .map_err(|_| StoreError::Unavailable)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(SqliteStoreError::from)?;
        sqlx::migrate!("./assets/persistence/mig")
            .run(&pool)
            .await
            .map_err(SqliteStoreError::from)?;
        Ok(Arc::new(Self { pool }))
    }
}

impl SqliteStore {
    pub fn pool_for_tests(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Store for Arc<SqliteStore> {
    async fn create_story_instance(
        &self,
        spec: &crate::persistence::store::MaterializedStoryInstanceSpec,
    ) -> Result<StoryInfo, StoreError> {
        Store::create_story_instance(&**self, spec).await
    }

    async fn get_story(&self, story_id: &StoryId) -> Result<Option<StoryInfo>, StoreError> {
        Store::get_story(&**self, story_id).await
    }

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError> {
        Store::load_story_snapshot(&**self, story_id, limits).await
    }

    async fn load_story_instance_meta(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<crate::persistence::store::StoryInstanceMeta>, StoreError> {
        Store::load_story_instance_meta(&**self, story_id).await
    }

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<crate::persistence::store::StoredTurnOutcome>, StoreError> {
        Store::find_committed_turn(&**self, story_id, idempotency_key).await
    }

    async fn commit_turn(&self, spec: &TurnCommitSpec) -> Result<CommittedTurnResult, StoreError> {
        Store::commit_turn(&**self, spec).await
    }
}

fn ensure_database_dir(url: &str) -> Result<(), StoreError> {
    if url.starts_with(':') {
        return Ok(());
    }
    let path = Path::new(url);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|_| StoreError::Unavailable)?;
        }
    }
    Ok(())
}

#[async_trait]
impl Store for SqliteStore {
    async fn create_story_instance(
        &self,
        spec: &crate::persistence::store::MaterializedStoryInstanceSpec,
    ) -> Result<StoryInfo, StoreError> {
        let mut tx = self.pool.begin().await.map_err(SqliteStoreError::from)?;
        let roles_json = serde_json::to_string(&spec.roles).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
        })?;
        let relationships_json = serde_json::to_string(&spec.relationships).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let narrative_state_json =
            serde_json::to_string(&spec.narrative_state).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let settings_json = serde_json::to_string(&spec.settings).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let fact_values_json = serde_json::to_string(&spec.fact_values).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let constraints_json =
            serde_json::to_string(&spec.active_constraints).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let player_role_id = spec
            .roles
            .values()
            .find(|role| role.is_player_controlled())
            .map(|role| role.role_id.as_str());
        sqlx::query(
            "INSERT INTO stories (id, revision, player_role_id, created_at, \
             story_summary, active_constraints) \
             VALUES (?, 0, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(player_role_id)
        .bind(spec.created_at_ms)
        .bind("{\"text\":\"\",\"summarized_through\":null}")
        .bind(&constraints_json)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        sqlx::query(
            "INSERT INTO story_instances \
             (story_id, pack_id, settings_json, roles_json, relationships_json, \
              narrative_state_json, fact_values_json, knowledge_id_high_water, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(spec.pack.pack_id.as_str())
        .bind(&settings_json)
        .bind(&roles_json)
        .bind(&relationships_json)
        .bind(&narrative_state_json)
        .bind(&fact_values_json)
        .bind(spec.knowledge_id_high_water.get() as i64)
        .bind(spec.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        for entry in &spec.knowledge {
            let source_id = entry.source_id();
            let payload_json = serde_json::to_string(entry).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            insert_knowledge_entry(
                &mut tx,
                KnowledgeEntryWrite {
                    story_id: &spec.story_id,
                    knowledge_kind: knowledge_kind_str(entry.kind()),
                    source_id: source_id.as_str(),
                    memory_owner: entry.memory_owner().map(RoleId::as_str),
                    content: entry.content().as_str(),
                    retrieval_hint: entry.retrieval_hint().map(|hint| hint.as_str()),
                    salience: entry.salience(),
                    source: entry.source(),
                    payload_json,
                    entities: entry.entities(),
                    topics: entry.topics(),
                },
            )
            .await?;
        }
        sqlx::query(
            "INSERT INTO story_segments (id, story_id, sequence, origin, turn_number, story_text, created_at) \
             VALUES (?, ?, 1, 'opening', NULL, ?, ?)",
        )
        .bind(format!("{}:opening", spec.story_id.as_str()))
        .bind(spec.story_id.as_str())
        .bind(spec.opening.as_str())
        .bind(spec.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(StoryInfo {
            story_id: spec.story_id.clone(),
            created_at_ms: spec.created_at_ms,
            base_revision: StoryRevision::new(0),
            last_committed_turn_number: 0,
        })
    }

    async fn get_story(&self, story_id: &StoryId) -> Result<Option<StoryInfo>, StoreError> {
        let row: Option<(i64, i64, i64)> =
            sqlx::query_as("SELECT revision, created_at, last_turn_number FROM stories WHERE id = ?")
                .bind(story_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStoreError::from)?;
        Ok(row.map(|(revision, created_at, last_turn_number)| StoryInfo {
            story_id: story_id.clone(),
            created_at_ms: created_at,
            base_revision: StoryRevision::new(revision as u64),
            last_committed_turn_number: last_turn_number as u64,
        }))
    }

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError> {
        sqlite_snapshot::load_story_snapshot(&self.pool, story_id, limits).await
    }

    async fn load_story_instance_meta(
        &self,
        story_id: &StoryId,
    ) -> Result<Option<crate::persistence::store::StoryInstanceMeta>, StoreError> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT pack_id, roles_json FROM story_instances WHERE story_id = ?")
                .bind(story_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStoreError::from)?;
        let Some((pack_id, roles_json)) = row else {
            return Ok(None);
        };
        let roles = serde_json::from_str(&roles_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
        })?;
        Ok(Some(crate::persistence::store::StoryInstanceMeta {
            pack_id: crate::domain::asset::ids::PackId::from(pack_id),
            roles,
        }))
    }

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoredTurnOutcome>, StoreError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT request_digest, result_json FROM story_turns WHERE story_id = ? AND idempotency_key = ?",
        )
        .bind(story_id.as_str())
        .bind(idempotency_key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStoreError::from)?;
        match row {
            Some((digest, result_json)) => Ok(Some(StoredTurnOutcome {
                request_digest: crate::turn::turn_contract::RequestDigest::from_stored(digest),
                result: serde_json::from_str(&result_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
                })?,
            })),
            None => Ok(None),
        }
    }

    async fn commit_turn(&self, commit: &TurnCommitSpec) -> Result<CommittedTurnResult, StoreError> {
        let mut tx = self.pool.begin().await.map_err(SqliteStoreError::from)?;
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT request_digest, result_json FROM story_turns WHERE story_id = ? AND idempotency_key = ?",
        )
        .bind(commit.story_id.as_str())
        .bind(commit.idempotency_key.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        if let Some((digest, result_json)) = existing {
            tx.rollback().await.map_err(SqliteStoreError::from)?;
            if digest == commit.request_digest.as_str() {
                return serde_json::from_str(&result_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
                });
            }
            return Err(StoreError::IdempotencyConflict);
        }

        let state: Option<(i64, i64, String, String, String)> = sqlx::query_as(
            "SELECT s.revision, s.last_turn_number, i.roles_json, i.relationships_json, i.narrative_state_json \
             FROM stories s JOIN story_instances i ON i.story_id = s.id WHERE s.id = ?",
        )
        .bind(commit.story_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let Some((stored_revision, stored_last_turn_number, roles_json, relationships_json, narrative_state_json)) =
            state
        else {
            return Err(StoreError::NotFound);
        };
        let base = commit.base_revision.get();
        if stored_revision < 0 || stored_revision as u64 != base {
            return Err(StoreError::RevisionConflict);
        }
        if stored_last_turn_number < 0 {
            return Err(StoreError::RevisionConflict);
        }
        let expected_turn_number = (stored_last_turn_number as u64)
            .checked_add(1)
            .ok_or(StoreError::LimitExceeded { limit: "turn_number" })?;
        if expected_turn_number != commit.turn.number.get() {
            return Err(StoreError::RevisionConflict);
        }
        let committed_revision = base.checked_add(1).ok_or(StoreError::LimitExceeded {
            limit: "story_revision",
        })?;
        let sequence: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) + 1 FROM story_segments WHERE story_id = ?")
                .bind(commit.story_id.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        if sequence <= 0 {
            return Err(StoreError::LimitExceeded { limit: "turn_sequence" });
        }
        let mut roles: std::collections::BTreeMap<RoleId, StoryRole> =
            serde_json::from_str(&roles_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
            })?;
        for role in commit.changes.new_roles() {
            if roles.contains_key(&role.role_id) {
                return Err(StoreError::ConstraintViolation {
                    constraint: "new_role_id_collision".to_owned(),
                });
            }
            roles.insert(role.role_id.clone(), role.clone());
        }
        for change in commit.changes.role_changes() {
            let Some(role) = roles.get_mut(&change.role_id) else {
                return Err(StoreError::ConstraintViolation {
                    constraint: "role_change_reference".to_owned(),
                });
            };
            role.state = change.new_state.clone();
        }
        let relationships: Vec<RelationshipState> =
            serde_json::from_str(&relationships_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let mut relationships_by_key: std::collections::BTreeMap<RelationshipKey, RelationshipState> =
            relationships.into_iter().map(|value| (value.key(), value)).collect();
        for operation in commit.changes.relationship_operations() {
            match operation {
                crate::turn::turn_validation::ValidatedRelationshipOperation::Add(state) => {
                    if relationships_by_key.contains_key(&state.key()) {
                        return Err(StoreError::ConstraintViolation {
                            constraint: "relationship_add_collision".to_owned(),
                        });
                    }
                    relationships_by_key.insert(state.key(), state.clone());
                }
                crate::turn::turn_validation::ValidatedRelationshipOperation::Update(change) => {
                    if !relationships_by_key.contains_key(&change.key) || change.key != change.new_state.key() {
                        return Err(StoreError::ConstraintViolation {
                            constraint: "relationship_change_reference".to_owned(),
                        });
                    }
                    relationships_by_key.insert(change.key.clone(), change.new_state.clone());
                }
            }
        }
        let mut narrative_state: NarrativeRuntimeState =
            serde_json::from_str(&narrative_state_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        if narrative_state.graph_revision != commit.expected_graph_revision {
            return Err(StoreError::RevisionConflict);
        }
        let resolution = commit.changes.narrative_resolution();
        for transition in &resolution.transitions {
            let expected_from = match transition.kind {
                crate::domain::narrative_graph::effect::NarrativeTransitionKind::Activate => {
                    crate::domain::narrative_graph::condition::NarrativeNodeState::Inactive
                }
                crate::domain::narrative_graph::effect::NarrativeTransitionKind::Complete
                | crate::domain::narrative_graph::effect::NarrativeTransitionKind::Skip => {
                    crate::domain::narrative_graph::condition::NarrativeNodeState::Active
                }
            };
            if narrative_state.node_state(&transition.node_key) != expected_from {
                return Err(StoreError::RevisionConflict);
            }
            let to_state = match transition.kind {
                crate::domain::narrative_graph::effect::NarrativeTransitionKind::Activate => {
                    crate::domain::narrative_graph::condition::NarrativeNodeState::Active
                }
                crate::domain::narrative_graph::effect::NarrativeTransitionKind::Complete => {
                    crate::domain::narrative_graph::condition::NarrativeNodeState::Completed
                }
                crate::domain::narrative_graph::effect::NarrativeTransitionKind::Skip => {
                    crate::domain::narrative_graph::condition::NarrativeNodeState::Skipped
                }
            };
            narrative_state.node_states.insert(transition.node_key.clone(), to_state);
            if to_state == crate::domain::narrative_graph::condition::NarrativeNodeState::Active {
                narrative_state
                    .activation_turns
                    .insert(transition.node_key.clone(), commit.turn.number);
            }
        }
        narrative_state.pending_effects = resolution
            .pending_effects
            .iter()
            .map(|effect| (effect.effect_id.clone(), effect.clone()))
            .collect();
        if !resolution.transitions.is_empty() {
            narrative_state.graph_revision = resolution.next_graph_revision;
        }
        let aggregate = aggregate_llm_usage(&commit.llm_calls)?;
        let result = CommittedTurnResult {
            turn_number: commit.turn.number,
            story_revision: StoryRevision::new(committed_revision),
            story_text: commit.changes.story_text().to_owned(),
            llm_usage: aggregate,
            llm_calls: commit.llm_calls.clone(),
        };
        let result_json = serde_json::to_string(&result).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
        })?;

        persist_instance_state(
            &mut tx,
            commit,
            &roles,
            relationships_by_key.into_values().collect(),
            &narrative_state,
            commit.changes.knowledge_id_high_water(),
            commit.changes.next_role_id_high_water(),
        )
        .await?;

        sqlx::query(
            "INSERT INTO story_turns (story_id, turn_number, player_input, story_text, status, created_at, \
             idempotency_key, request_digest, base_revision, committed_revision, result_json, sequence) \
             VALUES (?, ?, ?, ?, 'ok', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(commit.story_id.as_str())
        .bind(commit.turn.number.get() as i64)
        .bind(&commit.turn.player_input)
        .bind(commit.changes.story_text())
        .bind(commit.turn.created_at)
        .bind(commit.idempotency_key.as_str())
        .bind(commit.request_digest.as_str())
        .bind(base as i64)
        .bind(committed_revision as i64)
        .bind(&result_json)
        .bind(sequence)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;

        sqlx::query(
            "INSERT INTO story_segments (id, story_id, sequence, origin, turn_number, story_text, created_at) \
             VALUES (?, ?, ?, 'turn', ?, ?, ?)",
        )
        .bind(format!("{}:turn:{}", commit.story_id.as_str(), commit.turn.number))
        .bind(commit.story_id.as_str())
        .bind(sequence)
        .bind(commit.turn.number.get() as i64)
        .bind(commit.changes.story_text())
        .bind(commit.turn.created_at)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;

        for (seq, event) in commit.changes.narrative_events().iter().enumerate() {
            if event.turn_number != commit.turn.number {
                return Err(StoreError::ConstraintViolation {
                    constraint: "event_turn_reference".to_owned(),
                });
            }
            let payload = serde_json::to_string(&event.payload).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidEventPayload,
            })?;
            sqlx::query(
                "INSERT INTO story_events (id, story_id, turn_number, seq, kind, payload) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(event.id.as_str())
            .bind(commit.story_id.as_str())
            .bind(event.turn_number.get() as i64)
            .bind(seq as i64)
            .bind(event.kind.as_str())
            .bind(&payload)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
        }

        if let StateChange::Replace(constraints) = commit.changes.constraint_change() {
            let constraints_json = serde_json::to_string(&constraints).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            sqlx::query("UPDATE stories SET active_constraints = ? WHERE id = ?")
                .bind(&constraints_json)
                .bind(commit.story_id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        for record in &commit.outbox {
            write_outbox(&mut tx, record).await?;
        }

        for mutation in commit.changes.knowledge_mutations() {
            apply_knowledge_mutation(&mut tx, &commit.story_id, mutation).await?;
        }

        let updated = sqlx::query(
            "UPDATE stories SET revision = ?, last_turn_number = ? WHERE id = ? AND revision = ? AND last_turn_number = ?",
        )
        .bind(committed_revision as i64)
        .bind(commit.turn.number.get() as i64)
        .bind(commit.story_id.as_str())
        .bind(base as i64)
        .bind(stored_last_turn_number)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?
        .rows_affected();
        if updated != 1 {
            return Err(StoreError::RevisionConflict);
        }

        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(result)
    }
}

fn aggregate_llm_usage(
    calls: &[crate::turn::turn_contract::LlmCallUsage],
) -> Result<crate::turn::turn_contract::LlmUsageAggregate, StoreError> {
    let mut aggregate = crate::turn::turn_contract::LlmUsageAggregate {
        llm_calls: 0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
    };
    for call in calls {
        aggregate.llm_calls = aggregate
            .llm_calls
            .checked_add(1)
            .ok_or(StoreError::LimitExceeded { limit: "llm_calls" })?;
        aggregate.input_tokens =
            aggregate
                .input_tokens
                .checked_add(call.input_tokens)
                .ok_or(StoreError::LimitExceeded {
                    limit: "llm_input_tokens",
                })?;
        aggregate.output_tokens =
            aggregate
                .output_tokens
                .checked_add(call.output_tokens)
                .ok_or(StoreError::LimitExceeded {
                    limit: "llm_output_tokens",
                })?;
        aggregate.total_tokens =
            aggregate
                .total_tokens
                .checked_add(call.total_tokens)
                .ok_or(StoreError::LimitExceeded {
                    limit: "llm_total_tokens",
                })?;
    }
    Ok(aggregate)
}

async fn persist_instance_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    commit: &TurnCommitSpec,
    roles: &std::collections::BTreeMap<RoleId, StoryRole>,
    relationships: Vec<RelationshipState>,
    narrative_state: &NarrativeRuntimeState,
    knowledge_id_high_water: crate::domain::knowledge::KnowledgeIdHighWater,
    role_id_high_water: crate::domain::ids::RoleIdHighWater,
) -> Result<(), StoreError> {
    let roles_json = serde_json::to_string(roles).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidRoleState,
    })?;
    let relationships_json = serde_json::to_string(&relationships).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let narrative_state_json = serde_json::to_string(narrative_state).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    sqlx::query(
        "UPDATE story_instances SET roles_json = ?, relationships_json = ?, \
         narrative_state_json = ?, knowledge_id_high_water = ?, role_id_high_water = ? WHERE story_id = ?",
    )
    .bind(&roles_json)
    .bind(&relationships_json)
    .bind(&narrative_state_json)
    .bind(knowledge_id_high_water.get() as i64)
    .bind(role_id_high_water.get() as i64)
    .bind(commit.story_id.as_str())
    .execute(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    Ok(())
}

fn knowledge_kind_str(kind: crate::domain::knowledge::KnowledgeKind) -> &'static str {
    match kind {
        crate::domain::knowledge::KnowledgeKind::Fact => "fact",
        crate::domain::knowledge::KnowledgeKind::Rumor => "rumor",
        crate::domain::knowledge::KnowledgeKind::Memory => "memory",
    }
}

struct KnowledgeEntryWrite<'a> {
    story_id: &'a StoryId,
    knowledge_kind: &'a str,
    source_id: &'a str,
    memory_owner: Option<&'a str>,
    content: &'a str,
    retrieval_hint: Option<&'a str>,
    salience: u8,
    source: &'a crate::domain::knowledge::KnowledgeSource,
    payload_json: String,
    entities: &'a [crate::domain::asset::entity::KnowledgeEntity],
    topics: &'a [crate::domain::asset::ids::TopicKey],
}

async fn insert_knowledge_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry: KnowledgeEntryWrite<'_>,
) -> Result<(), StoreError> {
    let source_json = serde_json::to_string(entry.source).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    sqlx::query(
        "INSERT INTO knowledge_entries \
         (story_id, source_id, knowledge_kind, memory_owner_role_id, content, retrieval_hint, salience, \
          source_json, payload_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.story_id.as_str())
    .bind(entry.source_id)
    .bind(entry.knowledge_kind)
    .bind(entry.memory_owner)
    .bind(entry.content)
    .bind(entry.retrieval_hint)
    .bind(i64::from(entry.salience))
    .bind(&source_json)
    .bind(&entry.payload_json)
    .execute(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    write_knowledge_entity_and_topic_rows(
        tx,
        entry.story_id,
        entry.knowledge_kind,
        entry.source_id,
        entry.entities,
        entry.topics,
    )
    .await
}

async fn write_knowledge_entity_and_topic_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    story_id: &StoryId,
    knowledge_kind: &str,
    source_id: &str,
    entities: &[crate::domain::asset::entity::KnowledgeEntity],
    topics: &[crate::domain::asset::ids::TopicKey],
) -> Result<(), StoreError> {
    for entity in entities {
        let (entity_kind, entity_key) = match entity {
            crate::domain::asset::entity::KnowledgeEntity::World(key) => ("world", key.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::Role(id) => ("role", id.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::Location(key) => ("location", key.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::Scene(key) => ("scene", key.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::NarrativeNode(key) => {
                ("narrative_node", key.as_str().to_owned())
            }
            crate::domain::asset::entity::KnowledgeEntity::Event(key) => ("event", key.as_str().to_owned()),
        };
        sqlx::query(
            "INSERT INTO knowledge_entry_entities \
             (story_id, knowledge_kind, source_id, entity_kind, entity_key) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(story_id.as_str())
        .bind(knowledge_kind)
        .bind(source_id)
        .bind(entity_kind)
        .bind(&entity_key)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStoreError::from)?;
    }
    for topic in topics {
        sqlx::query(
            "INSERT INTO knowledge_entry_topics (story_id, knowledge_kind, source_id, topic_key) VALUES (?, ?, ?, ?)",
        )
        .bind(story_id.as_str())
        .bind(knowledge_kind)
        .bind(source_id)
        .bind(topic.as_str())
        .execute(&mut **tx)
        .await
        .map_err(SqliteStoreError::from)?;
    }
    Ok(())
}

async fn apply_knowledge_mutation(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    story_id: &StoryId,
    mutation: &crate::turn::turn_validation::ValidatedKnowledgeMutation,
) -> Result<(), StoreError> {
    use crate::turn::turn_validation::ValidatedKnowledgeOperation;
    match &mutation.operation {
        ValidatedKnowledgeOperation::Add(entry) => {
            let source_id_value = entry.source_id();
            let payload_json = serde_json::to_string(entry).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            insert_knowledge_entry(
                tx,
                KnowledgeEntryWrite {
                    story_id,
                    knowledge_kind: knowledge_kind_str(entry.kind()),
                    source_id: source_id_value.as_str(),
                    memory_owner: entry.memory_owner().map(RoleId::as_str),
                    content: entry.content().as_str(),
                    retrieval_hint: entry.retrieval_hint().map(|hint| hint.as_str()),
                    salience: entry.salience(),
                    source: entry.source(),
                    payload_json,
                    entities: entry.entities(),
                    topics: entry.topics(),
                },
            )
            .await
        }
        ValidatedKnowledgeOperation::Update { target, value } => {
            let knowledge_kind = knowledge_kind_str(value.kind());
            let source_id_str = target.as_str().to_owned();
            let existing_payload: Option<String> = sqlx::query_scalar(
                "SELECT payload_json FROM knowledge_entries WHERE story_id = ? AND knowledge_kind = ? AND source_id = ?",
            )
            .bind(story_id.as_str())
            .bind(knowledge_kind)
            .bind(&source_id_str)
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqliteStoreError::from)?;
            let Some(existing_payload) = existing_payload else {
                return Err(StoreError::ConstraintViolation {
                    constraint: "knowledge_update_target_missing".to_owned(),
                });
            };
            let existing: crate::domain::knowledge::KnowledgeEntry =
                serde_json::from_str(&existing_payload).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?;
            let merged = merge_knowledge_update(existing, value.clone())?;
            sqlx::query(
                "DELETE FROM knowledge_entry_entities WHERE story_id = ? AND knowledge_kind = ? AND source_id = ?",
            )
            .bind(story_id.as_str())
            .bind(knowledge_kind)
            .bind(&source_id_str)
            .execute(&mut **tx)
            .await
            .map_err(SqliteStoreError::from)?;
            sqlx::query(
                "DELETE FROM knowledge_entry_topics WHERE story_id = ? AND knowledge_kind = ? AND source_id = ?",
            )
            .bind(story_id.as_str())
            .bind(knowledge_kind)
            .bind(&source_id_str)
            .execute(&mut **tx)
            .await
            .map_err(SqliteStoreError::from)?;
            let payload_json = serde_json::to_string(&merged).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            let source_json = serde_json::to_string(merged.source()).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            let updated = sqlx::query(
                "UPDATE knowledge_entries SET content = ?, salience = ?, source_json = ?, payload_json = ? \
                 WHERE story_id = ? AND knowledge_kind = ? AND source_id = ?",
            )
            .bind(merged.content().as_str())
            .bind(i64::from(merged.salience()))
            .bind(&source_json)
            .bind(&payload_json)
            .bind(story_id.as_str())
            .bind(knowledge_kind)
            .bind(&source_id_str)
            .execute(&mut **tx)
            .await
            .map_err(SqliteStoreError::from)?
            .rows_affected();
            if updated != 1 {
                return Err(StoreError::ConstraintViolation {
                    constraint: "knowledge_update_target_missing".to_owned(),
                });
            }
            write_knowledge_entity_and_topic_rows(
                tx,
                story_id,
                knowledge_kind,
                &source_id_str,
                merged.entities(),
                merged.topics(),
            )
            .await
        }
        ValidatedKnowledgeOperation::Delete { target } => {
            let (knowledge_kind, source_id_str) = match target {
                crate::domain::turn::DeletableKnowledgeId::Rumor(id) => ("rumor", id.as_str().to_owned()),
                crate::domain::turn::DeletableKnowledgeId::Memory(id) => ("memory", id.as_str().to_owned()),
            };
            let deleted = sqlx::query(
                "DELETE FROM knowledge_entries WHERE story_id = ? AND knowledge_kind = ? AND source_id = ?",
            )
            .bind(story_id.as_str())
            .bind(knowledge_kind)
            .bind(&source_id_str)
            .execute(&mut **tx)
            .await
            .map_err(SqliteStoreError::from)?
            .rows_affected();
            if deleted != 1 {
                return Err(StoreError::ConstraintViolation {
                    constraint: "knowledge_delete_target_missing".to_owned(),
                });
            }
            Ok(())
        }
    }
}

fn merge_knowledge_update(
    existing: crate::domain::knowledge::KnowledgeEntry,
    value: crate::domain::knowledge::KnowledgeEntry,
) -> Result<crate::domain::knowledge::KnowledgeEntry, StoreError> {
    use crate::domain::knowledge::KnowledgeEntry;
    match (existing, value) {
        (KnowledgeEntry::Fact(old), KnowledgeEntry::Fact(new)) => {
            Ok(KnowledgeEntry::Fact(crate::domain::knowledge::fact::WorldFact {
                id: old.id,
                key: old.key,
                text: new.text,
                proposition: new.proposition,
                retrieval_hint: new.retrieval_hint,
                entities: new.entities,
                topics: new.topics,
                salience: new.salience,
                source: new.source,
            }))
        }
        (KnowledgeEntry::Rumor(old), KnowledgeEntry::Rumor(new)) => {
            Ok(KnowledgeEntry::Rumor(crate::domain::knowledge::rumor::SharedRumor {
                id: old.id,
                key: old.key,
                content: new.content,
                claim: new.claim,
                retrieval_hint: new.retrieval_hint,
                entities: new.entities,
                topics: new.topics,
                salience: new.salience,
                source_role_id: new.source_role_id,
                truth_value: new.truth_value,
                source: new.source,
            }))
        }
        (KnowledgeEntry::Memory(old), KnowledgeEntry::Memory(new)) => {
            if old.owner != new.owner {
                return Err(StoreError::ConstraintViolation {
                    constraint: "knowledge_memory_owner_immutable".to_owned(),
                });
            }
            Ok(KnowledgeEntry::Memory(crate::domain::knowledge::memory::MemoryEntry {
                id: old.id,
                owner: old.owner,
                kind: new.kind,
                content: new.content,
                entities: new.entities,
                topics: new.topics,
                salience: new.salience,
                source: new.source,
                created_at_ms: old.created_at_ms,
            }))
        }
        _ => Err(StoreError::ConstraintViolation {
            constraint: "knowledge_update_kind_mismatch".to_owned(),
        }),
    }
}

async fn write_outbox(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, record: &OutboxRecord) -> Result<(), StoreError> {
    let payload = serde_json::to_string(&record.payload).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidEventPayload,
    })?;
    sqlx::query(
        "INSERT INTO outbox (id, story_id, turn_number, event_type, payload, created_at, attempt_count, published_at, last_error) \
         VALUES (?, ?, ?, ?, ?, ?, 0, NULL, NULL)",
    )
    .bind(&record.id)
    .bind(record.story_id.as_str())
    .bind(record.turn_number.get() as i64)
    .bind(&record.event_type)
    .bind(&payload)
    .bind(record.created_at)
    .execute(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    Ok(())
}
