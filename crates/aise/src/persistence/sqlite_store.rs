use crate::core::turn_contract::{CommittedTurnResult, IdempotencyKey, StoryRevision};
use crate::core::turn_data::SnapshotLimits;
use crate::core::turn_validation::StateChange;
use crate::domain::ids::{CharacterId, FactId, MemoryId, StoryId, TurnId};
use crate::domain::memory::{MemoryEntry, MemoryKind};
use crate::domain::narrative::StoryTurn;
use crate::domain::story_state::{StoryCreateSpec, StoryInfo, StoryReadSnapshot};
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::store::{OutboxRecord, Store, StoreError, StoredTurnOutcome, TurnCommitSpec};
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
    async fn create_story(&self, spec: &StoryCreateSpec) -> Result<StoryInfo, StoreError> {
        Store::create_story(&**self, spec).await
    }

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

fn memory_kind_str(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Observed => "observed",
        MemoryKind::Inferred => "inferred",
        MemoryKind::Secret => "secret",
    }
}

fn memory_kind_from_str(s: &str) -> MemoryKind {
    match s {
        "observed" => MemoryKind::Observed,
        "inferred" => MemoryKind::Inferred,
        _ => MemoryKind::Secret,
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn create_story(&self, spec: &StoryCreateSpec) -> Result<StoryInfo, StoreError> {
        let story_config = serde_json::to_string(&spec.story_config).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let current_scene = serde_json::to_string(&spec.current_scene).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let story_summary = serde_json::to_string(&spec.story_summary).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        let active_constraints =
            serde_json::to_string(&spec.active_constraints).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let initial_world = match &spec.initial_world {
            Some(world) => {
                let state = serde_json::to_string(world).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
                })?;
                sqlx::query("INSERT INTO worlds (id, name, state, created_at) VALUES (?, ?, ?, ?)")
                    .bind(world.id.as_str())
                    .bind(&world.name)
                    .bind(&state)
                    .bind(spec.created_at_ms)
                    .execute(&self.pool)
                    .await
                    .map_err(SqliteStoreError::from)?;
                world.id.as_str().to_owned()
            }
            None => spec.story_id.as_str().to_owned(),
        };
        sqlx::query(
            "INSERT INTO stories (id, revision, player_character_id, created_at, story_instructions, story_config, \
             current_scene, story_summary, active_constraints) \
             VALUES (?, 0, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(spec.player_character_id.as_ref().map(|id| id.as_str()))
        .bind(spec.created_at_ms)
        .bind(&spec.story_instructions)
        .bind(&story_config)
        .bind(&current_scene)
        .bind(&story_summary)
        .bind(&active_constraints)
        .execute(&self.pool)
        .await
        .map_err(SqliteStoreError::from)?;
        let _ = initial_world;
        Ok(StoryInfo {
            story_id: spec.story_id.clone(),
            created_at_ms: spec.created_at_ms,
            base_revision: StoryRevision::new(0),
        })
    }

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
        let facts_json = serde_json::to_string(&spec.facts).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
        })?;
        let rumors_json = serde_json::to_string(&spec.rumors).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
        })?;
        let memories_json = serde_json::to_string(&spec.memories).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidMemory,
        })?;
        let narrative_state_json =
            serde_json::to_string(&spec.narrative_state).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
        let opening_text = spec.opening.to_string();
        let scene_json = serde_json::to_string(&spec.scene).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
        })?;
        sqlx::query(
            "INSERT INTO stories (id, revision, player_character_id, created_at, story_instructions, story_config, \
             current_scene, story_summary, active_constraints) \
             VALUES (?, 0, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(None::<String>)
        .bind(spec.created_at_ms)
        .bind(&opening_text)
        .bind("{}")
        .bind(&scene_json)
        .bind("{\"text\":\"\"}")
        .bind("[]")
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let initial_world_state =
            serde_json::json!({"id": spec.story_id.as_str(), "name": "materialized", "facts": []});
        sqlx::query("INSERT INTO worlds (id, name, state, created_at) VALUES (?, ?, ?, ?)")
            .bind(spec.story_id.as_str())
            .bind("materialized")
            .bind(initial_world_state.to_string())
            .bind(spec.created_at_ms)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
        sqlx::query(
            "INSERT INTO story_instances \
             (story_id, pack_id, revision, bindings_json, characters_json, relationships_json, facts_json, \
              rumors_json, memories_json, narrative_state_json, created_at_ms) \
             VALUES (?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(spec.story_id.as_str())
        .bind(spec.pack.pack_id.as_str())
        .bind(&bindings_json)
        .bind(&characters_json)
        .bind(&relationships_json)
        .bind(&facts_json)
        .bind(&rumors_json)
        .bind(&memories_json)
        .bind(&narrative_state_json)
        .bind(spec.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
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
        let mut tx = self.pool.begin().await.map_err(SqliteStoreError::from)?;
        let story: Option<(i64, Option<String>, String, String, String, String, String)> = sqlx::query_as(
            "SELECT revision, player_character_id, story_instructions, story_config, \
             current_scene, story_summary, active_constraints \
             FROM stories WHERE id = ?",
        )
        .bind(story_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;
        let Some((
            revision,
            player_character_id,
            story_instructions,
            story_config,
            current_scene,
            story_summary,
            active_constraints,
        )) = story
        else {
            tx.rollback().await.map_err(SqliteStoreError::from)?;
            return Err(StoreError::NotFound);
        };
        let player_character_id = player_character_id.map(CharacterId::from);

        let world: Option<crate::domain::world::WorldState> =
            match sqlx::query_as::<_, (String,)>("SELECT state FROM worlds WHERE id = ?")
                .bind(story_id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?
            {
                Some((state,)) => serde_json::from_str(&state).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
                })?,
                None => None,
            };

        let characters: Vec<crate::domain::character::CharacterState> =
            sqlx::query_as::<_, (String,)>("SELECT state FROM characters WHERE world_id = ? ORDER BY rowid LIMIT ?")
                .bind(story_id.as_str())
                .bind(limits.max_characters() as i64)
                .fetch_all(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?
                .into_iter()
                .map(|(state,)| {
                    serde_json::from_str(&state).map_err(|_| StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
                    })
                })
                .collect::<Result<_, _>>()?;

        let recent_turns: Vec<StoryTurn> = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT id, player_input, story_text, created_at FROM (\
             SELECT id, player_input, story_text, created_at, rowid AS _row \
             FROM story_turns \
             WHERE world_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?\
             ) ORDER BY created_at ASC, _row ASC",
        )
        .bind(story_id.as_str())
        .bind(limits.max_recent_turns() as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?
        .into_iter()
        .map(|(id, player_input, story_text, created_at)| {
            let id = TurnId::try_new(id).map_err(|_| StoreError::Unavailable)?;
            Ok(StoryTurn {
                id,
                player_input,
                story_text,
                created_at,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;

        let player_memories: Vec<MemoryEntry> = match &player_character_id {
            Some(character_id) => load_memories(&mut tx, character_id, limits.max_memories()).await?,
            None => Vec::new(),
        };

        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(StoryReadSnapshot::new(
            story_id.clone(),
            StoryRevision::new(revision as u64),
            crate::domain::story_state::AuthoritativeStoryState {
                story_instructions,
                story_config: serde_json::from_str(&story_config).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?,
                current_scene: serde_json::from_str(&current_scene).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?,
                story_summary: serde_json::from_str(&story_summary).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                })?,
                active_constraints: serde_json::from_str(&active_constraints).map_err(|_| {
                    StoreError::Serialization {
                        kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
                    }
                })?,
            },
            crate::domain::story_state::PlayerStoryState {
                player_character_id,
                player_memories,
            },
            world,
            characters,
            recent_turns,
        ))
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
                request_digest: crate::core::turn_contract::RequestDigest::from_stored(digest),
                result: serde_json::from_str(&result_json).map_err(|_| StoreError::Serialization {
                    kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
                })?,
            })),
            None => Ok(None),
        }
    }

    async fn commit_turn(&self, commit: &TurnCommitSpec) -> Result<CommittedTurnResult, StoreError> {
        let mut tx = self.pool.begin().await.map_err(SqliteStoreError::from)?;
        sqlx::query("UPDATE stories SET revision = revision WHERE id = ?")
            .bind(commit.story_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;

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

        let base = commit.base_revision.get();
        let updated = sqlx::query("UPDATE stories SET revision = revision + 1 WHERE id = ? AND revision = ?")
            .bind(commit.story_id.as_str())
            .bind(base as i64)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?
            .rows_affected();
        if updated == 0 {
            tx.rollback().await.map_err(SqliteStoreError::from)?;
            return Err(StoreError::RevisionConflict);
        }

        let committed_revision = base.saturating_add(1);
        let aggregate = commit.llm_calls.iter().fold(
            crate::core::turn_contract::LlmUsageAggregate {
                llm_calls: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
            },
            |mut aggregate, call| {
                aggregate.llm_calls = aggregate.llm_calls.saturating_add(1);
                aggregate.input_tokens = aggregate.input_tokens.saturating_add(call.input_tokens);
                aggregate.output_tokens = aggregate.output_tokens.saturating_add(call.output_tokens);
                aggregate.total_tokens = aggregate.total_tokens.saturating_add(call.total_tokens);
                aggregate
            },
        );
        let result = CommittedTurnResult {
            turn_id: commit.turn.id.clone(),
            story_revision: StoryRevision::new(committed_revision),
            story_text: commit.turn.story_text.clone(),
            llm_usage: aggregate,
            llm_calls: commit.llm_calls.clone(),
        };
        let result_json = serde_json::to_string(&result).map_err(|_| StoreError::Serialization {
            kind: crate::persistence::store::StoreSerializationErrorKind::InvalidTurnResult,
        })?;

        if let StateChange::Replace(world) = &commit.world_change {
            let world_state = serde_json::to_string(world).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidWorldState,
            })?;
            sqlx::query(
                "INSERT INTO worlds (id, name, state, created_at) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = excluded.state",
            )
            .bind(world.id.as_str())
            .bind(&world.name)
            .bind(&world_state)
            .bind(commit.turn.created_at)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
        }

        sqlx::query(
            "INSERT INTO story_turns (id, world_id, player_input, story_text, status, created_at, \
             idempotency_key, request_digest, base_revision, committed_revision, result_json) \
             VALUES (?, ?, ?, ?, 'ok', ?, ?, ?, ?, ?, ?)",
        )
        .bind(commit.turn.id.as_str())
        .bind(commit.story_id.as_str())
        .bind(&commit.turn.player_input)
        .bind(&commit.turn.story_text)
        .bind(commit.turn.created_at)
        .bind(commit.idempotency_key.as_str())
        .bind(commit.request_digest.as_str())
        .bind(base as i64)
        .bind(committed_revision as i64)
        .bind(&result_json)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStoreError::from)?;

        for (seq, event) in commit.events.iter().enumerate() {
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

        for change in &commit.character_changes {
            let state = serde_json::to_string(&change.new_state).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidCharacterState,
            })?;
            sqlx::query(
                "INSERT INTO characters (id, world_id, state, created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
            )
            .bind(change.character_id.as_str())
            .bind(commit.story_id.as_str())
            .bind(&state)
            .bind(commit.turn.created_at)
            .bind(commit.turn.created_at)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStoreError::from)?;
        }

        for change in &commit.memory_changes {
            sqlx::query("INSERT INTO memory (id, character_id, kind, content, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(change.entry.id.as_str())
                .bind(change.entry.owner.as_str())
                .bind(memory_kind_str(change.entry.kind))
                .bind(&change.entry.content)
                .bind(change.entry.created_at)
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        if let StateChange::Replace(scene) = &commit.scene_change {
            let scene_json = serde_json::to_string(scene).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            sqlx::query("UPDATE stories SET current_scene = ? WHERE id = ?")
                .bind(&scene_json)
                .bind(commit.story_id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        if let StateChange::Replace(constraints) = &commit.constraint_change {
            let constraints_json = serde_json::to_string(constraints).map_err(|_| StoreError::Serialization {
                kind: crate::persistence::store::StoreSerializationErrorKind::InvalidStoryState,
            })?;
            sqlx::query("UPDATE stories SET active_constraints = ? WHERE id = ?")
                .bind(&constraints_json)
                .bind(commit.story_id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(SqliteStoreError::from)?;
        }

        if let StateChange::Replace(summary) = &commit.summary_change {
            let summary_json = serde_json::to_string(summary).map_err(|_| StoreError::Serialization {
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

        tx.commit().await.map_err(SqliteStoreError::from)?;
        Ok(result)
    }
}

async fn load_memories(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    character_id: &CharacterId,
    limit: usize,
) -> Result<Vec<MemoryEntry>, StoreError> {
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, kind, content, created_at FROM memory WHERE character_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(character_id.as_str())
    .bind(limit as i64)
    .fetch_all(&mut **tx)
    .await
    .map_err(SqliteStoreError::from)?;
    rows.into_iter()
        .map(|(id, kind, content, created_at)| {
            Ok(MemoryEntry {
                id: MemoryId::from(id),
                owner: character_id.clone(),
                kind: memory_kind_from_str(&kind),
                content,
                created_at,
            })
        })
        .collect()
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

#[allow(dead_code)]
fn _fact_id(_value: FactId) {}
