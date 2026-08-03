use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::{MemoryEntry, MemoryKind};
use crate::domain::narrative::{EventKind, StoryTurn};
use crate::domain::world::WorldState;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(url: &str) -> Result<Arc<Self>, AiseError> {
        ensure_database_dir(url)?;

        let options = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
        sqlx::migrate!("./assets/persistence/mig").run(&pool).await?;
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

fn event_kind_str(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Dialogue => "dialogue",
        EventKind::Action => "action",
        EventKind::WorldChange => "world_change",
        EventKind::Chapter => "chapter",
    }
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
    async fn load_world(&self, story_id: &StoryId) -> Result<Option<WorldState>, AiseError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT state FROM worlds WHERE id = ?")
            .bind(story_id.as_str())
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some((state,)) => Ok(Some(serde_json::from_str(&state)?)),
            None => Ok(None),
        }
    }

    async fn load_characters(&self, story_id: &StoryId) -> Result<Vec<CharacterState>, AiseError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT state FROM characters WHERE world_id = ?")
            .bind(story_id.as_str())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(state,)| serde_json::from_str(&state).map_err(AiseError::from))
            .collect()
    }

    async fn load_memory(&self, character_id: &CharacterId, limit: usize) -> Result<Vec<MemoryEntry>, AiseError> {
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT id, kind, content, created_at FROM memory WHERE character_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(character_id.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
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

    async fn load_story(&self, story_id: &StoryId, limit: usize) -> Result<Vec<StoryTurn>, AiseError> {
        let rows: Vec<(String, String, String, Option<String>, i64)> = sqlx::query_as(
            "SELECT id, player_input, story_text, summary_delta, created_at FROM story_turns \
             WHERE world_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(story_id.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, player_input, story_text, summary_delta, created_at)| {
                Ok(StoryTurn {
                    id: crate::domain::ids::TurnId::from(id),
                    player_input,
                    story_text,
                    summary_delta,
                    created_at,
                })
            })
            .collect()
    }

    async fn commit_turn(&self, commit: &TurnCommit) -> Result<(), AiseError> {
        let mut tx = self.pool.begin().await?;

        let world = match &commit.world {
            Some(w) => w.clone(),
            None => WorldState {
                id: commit.story_id.clone(),
                name: String::new(),
                facts: Vec::new(),
                characters: Vec::new(),
            },
        };
        let world_state = serde_json::to_string(&world)?;
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

        sqlx::query(
            "INSERT INTO story_turns (id, world_id, player_input, story_text, summary_delta, status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(commit.turn.id.as_str())
        .bind(commit.story_id.as_str())
        .bind(&commit.turn.player_input)
        .bind(&commit.turn.story_text)
        .bind(&commit.turn.summary_delta)
        .bind("ok")
        .bind(commit.turn.created_at)
        .execute(&mut *tx)
        .await?;

        for (seq, event) in commit.events.iter().enumerate() {
            let payload = serde_json::to_string(&event.payload)?;
            sqlx::query("INSERT INTO story_events (id, turn_id, seq, kind, payload) VALUES (?, ?, ?, ?, ?)")
                .bind(event.id.as_str())
                .bind(event.turn_id.as_str())
                .bind(seq as i64)
                .bind(event_kind_str(event.kind))
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

        tx.commit().await?;
        Ok(())
    }
}
