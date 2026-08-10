use crate::domain::ids::{CharacterId, StoryId, StoryRevision};
use crate::domain::narrative_graph::state::NarrativeRuntimeState;
use crate::domain::story_instance::info::StoryInfo;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::story_instance::state::{CharacterInstanceState, RelationshipKey, RelationshipState};
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
        let bindings_json = serde_json::to_string(&spec.bindings).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let characters_json = serde_json::to_string(&spec.characters).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
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
        let current_perceptions_json =
            serde_json::to_string(&spec.current_perceptions).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let condition_state_json =
            serde_json::to_string(&spec.condition_state).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let scene_json = serde_json::to_string(&spec.scene).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let constraints_json =
            serde_json::to_string(&spec.active_constraints).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let player_character_id = spec
            .bindings
            .values()
            .find(|binding| binding.is_player_controlled())
            .map(|binding| binding.character_id.as_str().to_owned());
        sqlx::query(
            "INSERT INTO stories (id, revision, player_character_id, created_at, \
             current_scene, story_summary, active_constraints) \
             VALUES (?, 0, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(player_character_id.as_deref())
        .bind(spec.created_at_ms)
        .bind(&scene_json)
        .bind("{\"text\":\"\",\"summarized_through\":null}")
        .bind(&constraints_json)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        sqlx::query(
            "INSERT INTO story_instances \
             (story_id, pack_id, settings_json, bindings_json, characters_json, relationships_json, \
              current_perceptions_json, narrative_state_json, condition_state_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(spec.pack.pack_id.as_str())
        .bind(&settings_json)
        .bind(&bindings_json)
        .bind(&characters_json)
        .bind(&relationships_json)
        .bind(&current_perceptions_json)
        .bind(&narrative_state_json)
        .bind(&condition_state_json)
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
                    memory_owner: entry.memory_owner().map(CharacterId::as_str),
                    content: entry.content().as_str(),
                    salience: entry.salience(),
                    source: entry.source(),
                    source_revision: entry.source_revision().get(),
                    payload_json,
                    entities: entry.entities(),
                    topics: entry.topics(),
                },
            )
            .await?;
        }
        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(StoryInfo {
            story_id: spec.story_id.clone(),
            created_at_ms: spec.created_at_ms,
            base_revision: StoryRevision::new(0),
        })
    }

    async fn get_story(&self, story_id: &StoryId) -> Result<Option<StoryInfo>, StoreError> {
        let row: Option<(i64, i64)> = sqlx::query_as("SELECT revision, created_at FROM stories WHERE id = ?")
            .bind(story_id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(SqliteStoreError::from)?;
        Ok(row.map(|(revision, created_at)| StoryInfo {
            story_id: story_id.clone(),
            created_at_ms: created_at,
            base_revision: StoryRevision::new(revision as u64),
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
        let row: Option<(String, String, String)> =
            sqlx::query_as("SELECT pack_id, bindings_json, characters_json FROM story_instances WHERE story_id = ?")
                .bind(story_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStoreError::from)?;
        let Some((pack_id, bindings_json, characters_json)) = row else {
            return Ok(None);
        };
        let bindings = serde_json::from_str(&bindings_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let characters = serde_json::from_str(&characters_json).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
        })?;
        Ok(Some(crate::persistence::store::StoryInstanceMeta {
            pack_id: crate::domain::asset::ids::PackId::from(pack_id),
            bindings,
            characters,
        }))
    }

    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoredTurnOutcome>, StoreError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT request_digest, result_json FROM story_turns WHERE world_id = ? AND idempotency_key = ?",
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
            "SELECT request_digest, result_json FROM story_turns WHERE world_id = ? AND idempotency_key = ?",
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

        let state: Option<(i64, String, String, String)> = sqlx::query_as(
            "SELECT s.revision, i.characters_json, i.relationships_json, i.narrative_state_json \
             FROM stories s JOIN story_instances i ON i.story_id = s.id WHERE s.id = ?",
        )
        .bind(commit.story_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let Some((stored_revision, characters_json, relationships_json, narrative_state_json)) = state else {
            return Err(StoreError::NotFound);
        };
        let base = commit.base_revision.get();
        if stored_revision < 0 || stored_revision as u64 != base {
            return Err(StoreError::RevisionConflict);
        }
        let committed_revision = base.checked_add(1).ok_or(StoreError::LimitExceeded {
            limit: "story_revision",
        })?;
        let sequence: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) + 1 FROM story_turns WHERE world_id = ?")
                .bind(commit.story_id.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        if sequence <= 0 {
            return Err(StoreError::LimitExceeded { limit: "turn_sequence" });
        }
        let mut characters: std::collections::BTreeMap<CharacterId, CharacterInstanceState> =
            serde_json::from_str(&characters_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
            })?;
        for change in commit.changes.character_changes() {
            if !characters.contains_key(&change.character_id) || change.character_id != change.new_state.character_id {
                return Err(StoreError::ConstraintViolation {
                    constraint: "character_change_reference".to_owned(),
                });
            }
            characters.insert(change.character_id.clone(), change.new_state.clone());
        }
        let relationships: Vec<RelationshipState> =
            serde_json::from_str(&relationships_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let mut relationships_by_key: std::collections::BTreeMap<RelationshipKey, RelationshipState> =
            relationships.into_iter().map(|value| (value.key(), value)).collect();
        for change in commit.changes.relationship_changes() {
            if !relationships_by_key.contains_key(&change.key) || change.key != change.new_state.key() {
                return Err(StoreError::ConstraintViolation {
                    constraint: "relationship_change_reference".to_owned(),
                });
            }
            relationships_by_key.insert(change.key.clone(), change.new_state.clone());
        }
        let mut narrative_state: NarrativeRuntimeState =
            serde_json::from_str(&narrative_state_json).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        if narrative_state.graph_revision != commit.expected_graph_revision {
            return Err(StoreError::RevisionConflict);
        }
        for change in commit.changes.narrative_changes() {
            if change.expected_graph_revision != commit.expected_graph_revision
                || narrative_state.node_state(&change.node_key) != change.from
            {
                return Err(StoreError::RevisionConflict);
            }
            narrative_state.node_states.insert(change.node_key.clone(), change.to);
            if change.to == crate::domain::narrative_graph::definition::NarrativeNodeState::Active {
                narrative_state
                    .activation_turns
                    .insert(change.node_key.clone(), commit.turn.id.clone());
            }
        }
        if !commit.changes.narrative_changes().is_empty() {
            narrative_state.graph_revision =
                narrative_state.graph_revision.checked_add(1).ok_or(StoreError::LimitExceeded {
                    limit: "narrative_graph_revision",
                })?;
        }
        let aggregate = aggregate_llm_usage(&commit.llm_calls)?;
        let result = CommittedTurnResult {
            turn_id: commit.turn.id.clone(),
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
            &characters,
            relationships_by_key.into_values().collect(),
            &narrative_state,
        )
        .await?;

        sqlx::query(
            "INSERT INTO story_turns (id, world_id, player_input, story_text, status, created_at, \
             idempotency_key, request_digest, base_revision, committed_revision, result_json, sequence) \
             VALUES (?, ?, ?, ?, 'ok', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(commit.turn.id.as_str())
        .bind(commit.story_id.as_str())
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

        for (seq, event) in commit.changes.events().iter().enumerate() {
            if event.turn_id != commit.turn.id {
                return Err(StoreError::ConstraintViolation {
                    constraint: "event_turn_reference".to_owned(),
                });
            }
            let payload = serde_json::to_string(&event.payload).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidEventPayload,
            })?;
            sqlx::query("INSERT INTO story_events (id, turn_id, seq, kind, payload) VALUES (?, ?, ?, ?, ?)")
                .bind(event.id.as_str())
                .bind(event.turn_id.as_str())
                .bind(seq as i64)
                .bind(event.kind.as_str())
                .bind(&payload)
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        if let StateChange::Replace(scene) = commit.changes.scene_change() {
            let scene_json = serde_json::to_string(&scene).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            sqlx::query("UPDATE stories SET current_scene = ? WHERE id = ?")
                .bind(&scene_json)
                .bind(commit.story_id.as_str())
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

        if let StateChange::Replace(summary) = commit.changes.summary_change() {
            let summary_json = serde_json::to_string(&summary).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            sqlx::query("UPDATE stories SET story_summary = ? WHERE id = ?")
                .bind(&summary_json)
                .bind(commit.story_id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        for record in &commit.outbox {
            write_outbox(&mut tx, record).await?;
        }

        for entry in commit.changes.knowledge_additions() {
            if entry.source_revision().get() != committed_revision {
                return Err(StoreError::ConstraintViolation {
                    constraint: "knowledge_source_revision".to_owned(),
                });
            }
            let source_id = entry.source_id();
            let payload_json = serde_json::to_string(entry).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            insert_knowledge_entry(
                &mut tx,
                KnowledgeEntryWrite {
                    story_id: &commit.story_id,
                    knowledge_kind: knowledge_kind_str(entry.kind()),
                    source_id: source_id.as_str(),
                    memory_owner: entry.memory_owner().map(CharacterId::as_str),
                    content: entry.content().as_str(),
                    salience: entry.salience(),
                    source: entry.source(),
                    source_revision: committed_revision,
                    payload_json,
                    entities: entry.entities(),
                    topics: entry.topics(),
                },
            )
            .await?;
        }

        let updated = sqlx::query("UPDATE stories SET revision = ? WHERE id = ? AND revision = ?")
            .bind(committed_revision as i64)
            .bind(commit.story_id.as_str())
            .bind(base as i64)
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
    characters: &std::collections::BTreeMap<CharacterId, CharacterInstanceState>,
    relationships: Vec<RelationshipState>,
    narrative_state: &NarrativeRuntimeState,
) -> Result<(), StoreError> {
    let characters_json = serde_json::to_string(characters).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
    })?;
    let relationships_json = serde_json::to_string(&relationships).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let perceptions_json =
        serde_json::to_string(commit.changes.current_perceptions()).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    let narrative_state_json = serde_json::to_string(narrative_state).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
    })?;
    let condition_state_json =
        serde_json::to_string(commit.changes.condition_state()).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
    sqlx::query(
        "UPDATE story_instances SET characters_json = ?, relationships_json = ?, current_perceptions_json = ?, \
         narrative_state_json = ?, condition_state_json = ? WHERE story_id = ?",
    )
    .bind(&characters_json)
    .bind(&relationships_json)
    .bind(&perceptions_json)
    .bind(&narrative_state_json)
    .bind(&condition_state_json)
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
    salience: u8,
    source: &'a crate::domain::knowledge::KnowledgeSource,
    source_revision: u64,
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
         (story_id, source_id, knowledge_kind, memory_owner_character_id, content, salience, source_json, payload_json, source_revision) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.story_id.as_str())
    .bind(entry.source_id)
    .bind(entry.knowledge_kind)
    .bind(entry.memory_owner)
    .bind(entry.content)
    .bind(i64::from(entry.salience))
    .bind(&source_json)
    .bind(&entry.payload_json)
    .bind(entry.source_revision as i64)
    .execute(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    for entity in entry.entities {
        let (entity_kind, entity_key) = match entity {
            crate::domain::asset::entity::KnowledgeEntity::World(key) => ("world", key.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::Role(key) => ("role", key.as_str().to_owned()),
            crate::domain::asset::entity::KnowledgeEntity::Character(id) => ("character", id.as_str().to_owned()),
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
        .bind(entry.story_id.as_str())
        .bind(entry.knowledge_kind)
        .bind(entry.source_id)
        .bind(entity_kind)
        .bind(&entity_key)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStoreError::from)?;
    }
    for topic in entry.topics {
        sqlx::query(
            "INSERT INTO knowledge_entry_topics (story_id, knowledge_kind, source_id, topic_key) VALUES (?, ?, ?, ?)",
        )
        .bind(entry.story_id.as_str())
        .bind(entry.knowledge_kind)
        .bind(entry.source_id)
        .bind(topic.as_str())
        .execute(&mut **tx)
        .await
        .map_err(SqliteStoreError::from)?;
    }
    Ok(())
}

async fn write_outbox(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, record: &OutboxRecord) -> Result<(), StoreError> {
    let payload = serde_json::to_string(&record.payload).map_err(|_| StoreError::Serialization {
        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidEventPayload,
    })?;
    sqlx::query(
        "INSERT INTO outbox (id, story_id, turn_id, event_type, payload, created_at, attempt_count, published_at, last_error) \
         VALUES (?, ?, ?, ?, ?, ?, 0, NULL, NULL)",
    )
    .bind(&record.id)
    .bind(record.story_id.as_str())
    .bind(record.turn_id.as_str())
    .bind(&record.event_type)
    .bind(&payload)
    .bind(record.created_at)
    .execute(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    Ok(())
}
