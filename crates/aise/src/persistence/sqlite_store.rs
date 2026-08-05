use crate::core::turn_contract::{CommittedTurnResult, IdempotencyKey, StoryRevision};
use crate::core::turn_data::{SnapshotLimits, StoryReadSnapshot};
use crate::core::turn_validation::StateChange;
use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId, TurnId};
use crate::domain::memory::{MemoryEntry, MemoryKind};
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use crate::error::AiseError;
use crate::persistence::store::{OutboxRecord, Store, StoreError, StoredTurnOutcome, TurnCommit};
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
    pub async fn connect(url: &str) -> Result<Arc<Self>, AiseError> {
        ensure_database_dir(url)?;

        let options = SqliteConnectOptions::from_str(url)
            .map_err(|error| AiseError::Store(StoreError::from(error)))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|error| AiseError::Store(StoreError::from(error)))?;
        sqlx::migrate!("./assets/persistence/mig")
            .run(&pool)
            .await
            .map_err(|error| AiseError::Store(StoreError::from(error)))?;
        Ok(Arc::new(Self { pool }))
    }
}

fn ensure_database_dir(url: &str) -> Result<(), AiseError> {
    if url.starts_with(':') {
        return Ok(());
    }
    let path = Path::new(url);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
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
    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<Option<StoryReadSnapshot>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let story: Option<(i64, Option<String>)> =
            sqlx::query_as("SELECT revision, player_character_id FROM stories WHERE id = ?")
                .bind(story_id.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        let Some((revision, player_character_id)) = story else {
            tx.rollback().await?;
            return Ok(None);
        };
        let player_character_id = player_character_id.map(CharacterId::from);

        let world: Option<WorldState> = match sqlx::query_as::<_, (String,)>("SELECT state FROM worlds WHERE id = ?")
            .bind(story_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            Some((state,)) => Some(serde_json::from_str(&state)?),
            None => None,
        };

        let characters: Vec<CharacterState> =
            sqlx::query_as::<_, (String,)>("SELECT state FROM characters WHERE world_id = ? ORDER BY rowid")
                .bind(story_id.as_str())
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|(state,)| serde_json::from_str(&state).map_err(StoreError::from))
                .collect::<Result<_, _>>()?;

        let recent_turns: Vec<StoryTurn> = sqlx::query_as::<_, (String, String, String, Option<String>, i64)>(
            "SELECT id, player_input, story_text, summary_delta, created_at FROM (\
             SELECT id, player_input, story_text, summary_delta, created_at, rowid AS _row \
             FROM story_turns \
             WHERE world_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?\
             ) ORDER BY created_at ASC, _row ASC",
        )
        .bind(story_id.as_str())
        .bind(limits.max_recent_turns as i64)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|(id, player_input, story_text, summary_delta, created_at)| {
            Ok(StoryTurn {
                id: TurnId::from(id),
                player_input,
                story_text,
                summary_delta,
                created_at,
            })
        })
        .collect::<Result<_, StoreError>>()?;

        let player_memories: Vec<MemoryEntry> = match &player_character_id {
            Some(character_id) => load_memories(&mut tx, character_id, limits.max_memories).await?,
            None => Vec::new(),
        };

        tx.commit().await?;
        Ok(Some(StoryReadSnapshot::new(
            story_id.clone(),
            StoryRevision::new(revision as u64),
            player_character_id,
            world,
            characters,
            recent_turns,
            player_memories,
        )))
    }

    async fn create_story(
        &self,
        story_id: &StoryId,
        player_character_id: Option<&CharacterId>,
        created_at: i64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO stories (id, revision, player_character_id, created_at) VALUES (?, 0, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
             player_character_id = COALESCE(stories.player_character_id, excluded.player_character_id)",
        )
        .bind(story_id.as_str())
        .bind(player_character_id.map(|id| id.as_str()))
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
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
        .await?;
        match row {
            Some((digest, result_json)) => Ok(Some(StoredTurnOutcome {
                request_digest: crate::core::turn_contract::RequestDigest::from_stored(digest),
                result: serde_json::from_str(&result_json)?,
            })),
            None => Ok(None),
        }
    }

    async fn commit_turn(&self, commit: &TurnCommit) -> Result<CommittedTurnResult, StoreError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO stories (id, revision, player_character_id, created_at) VALUES (?, 0, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET revision = stories.revision",
        )
        .bind(commit.story_id.as_str())
        .bind(commit.player_character_id.as_ref().map(|id| id.as_str()))
        .bind(commit.turn.created_at)
        .execute(&mut *tx)
        .await?;

        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT request_digest, result_json FROM story_turns WHERE world_id = ? AND idempotency_key = ?",
        )
        .bind(commit.story_id.as_str())
        .bind(commit.idempotency_key.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((digest, result_json)) = existing {
            tx.rollback().await?;
            if digest == commit.request_digest.as_str() {
                return serde_json::from_str(&result_json).map_err(StoreError::from);
            }
            return Err(StoreError::IdempotencyConflict);
        }

        let base = commit.base_revision.get();
        let updated = sqlx::query("UPDATE stories SET revision = revision + 1 WHERE id = ? AND revision = ?")
            .bind(commit.story_id.as_str())
            .bind(base as i64)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            if base > 0 {
                tx.rollback().await?;
                return Err(StoreError::RevisionConflict);
            }
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO stories (id, revision, player_character_id, created_at) VALUES (?, 1, ?, ?)",
            )
            .bind(commit.story_id.as_str())
            .bind(commit.player_character_id.as_ref().map(|id| id.as_str()))
            .bind(commit.turn.created_at)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if inserted == 0 {
                tx.rollback().await?;
                return Err(StoreError::RevisionConflict);
            }
        }

        let committed_revision = base.saturating_add(1);
        let result = CommittedTurnResult {
            turn_id: commit.turn.id.clone(),
            story_revision: StoryRevision::new(committed_revision),
            story_text: commit.turn.story_text.clone(),
            llm_usage: commit.llm_usage,
        };
        let result_json = serde_json::to_string(&result)?;

        if let StateChange::Replace(world) = &commit.world {
            let world_state = serde_json::to_string(world)?;
            sqlx::query(
                "INSERT INTO worlds (id, name, state, created_at) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = excluded.state",
            )
            .bind(world.id.as_str())
            .bind(&world.name)
            .bind(&world_state)
            .bind(commit.turn.created_at)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "INSERT INTO story_turns (id, world_id, player_input, story_text, summary_delta, status, created_at, \
             idempotency_key, request_digest, base_revision, committed_revision, result_json) \
             VALUES (?, ?, ?, ?, ?, 'ok', ?, ?, ?, ?, ?, ?)",
        )
        .bind(commit.turn.id.as_str())
        .bind(commit.story_id.as_str())
        .bind(&commit.turn.player_input)
        .bind(&commit.turn.story_text)
        .bind(&commit.turn.summary_delta)
        .bind(commit.turn.created_at)
        .bind(commit.idempotency_key.as_str())
        .bind(commit.request_digest.as_str())
        .bind(base as i64)
        .bind(committed_revision as i64)
        .bind(&result_json)
        .execute(&mut *tx)
        .await?;

        for (seq, event) in commit.events.iter().enumerate() {
            let payload = serde_json::to_string(&event.payload)?;
            sqlx::query("INSERT INTO story_events (id, turn_id, seq, kind, payload) VALUES (?, ?, ?, ?, ?)")
                .bind(event.id.as_str())
                .bind(event.turn_id.as_str())
                .bind(seq as i64)
                .bind(event.kind.as_str())
                .bind(&payload)
                .execute(&mut *tx)
                .await?;
        }

        for character in &commit.characters {
            let state = serde_json::to_string(character)?;
            sqlx::query(
                "INSERT INTO characters (id, world_id, state, created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
            )
            .bind(character.id.as_str())
            .bind(commit.story_id.as_str())
            .bind(&state)
            .bind(commit.turn.created_at)
            .bind(commit.turn.created_at)
            .execute(&mut *tx)
            .await?;
        }

        for memory in &commit.memory {
            sqlx::query("INSERT INTO memory (id, character_id, kind, content, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(memory.id.as_str())
                .bind(memory.owner.as_str())
                .bind(memory_kind_str(memory.kind))
                .bind(&memory.content)
                .bind(memory.created_at)
                .execute(&mut *tx)
                .await?;
        }

        for record in &commit.outbox {
            write_outbox(&mut tx, record).await?;
        }

        tx.commit().await?;
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
    .await?;
    rows.into_iter()
        .map(|(id, kind, content, created_at)| {
            Ok(MemoryEntry {
                id: crate::domain::ids::MemoryId::from(id),
                owner: character_id.clone(),
                kind: memory_kind_from_str(&kind),
                content,
                created_at,
            })
        })
        .collect()
}

async fn write_outbox(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, record: &OutboxRecord) -> Result<(), StoreError> {
    let payload = serde_json::to_string(&record.payload)?;
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
    .await?;
    Ok(())
}
