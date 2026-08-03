use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};

/// SQLite-backed `Store`. Migrations are embedded and applied at connect time.
pub struct SqliteStore {
    #[allow(dead_code)] // pool is exercised once load/commit query bodies are implemented
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(url: &str) -> Result<Arc<Self>, AiseError> {
        ensure_database_dir(url)?;
        // sqlx does not create missing files by default; flip that on for
        // first-run ergonomics.
        let options = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Arc::new(Self { pool }))
    }
}

/// Creates the database file's parent directory so `data/aise.db` works on
/// first run. `:memory:` has no path and is skipped.
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

#[async_trait]
impl Store for SqliteStore {
    async fn load_world(&self, story_id: &StoryId) -> Result<Option<WorldState>, AiseError> {
        // Framework stub.
        let _ = story_id;
        Ok(None)
    }

    async fn load_characters(&self, story_id: &StoryId) -> Result<Vec<CharacterState>, AiseError> {
        let _ = story_id;
        Ok(Vec::new())
    }

    async fn load_memory(&self, character_id: &CharacterId, limit: usize) -> Result<Vec<MemoryEntry>, AiseError> {
        let _ = (character_id, limit);
        Ok(Vec::new())
    }

    async fn load_story(&self, story_id: &StoryId, limit: usize) -> Result<Vec<StoryTurn>, AiseError> {
        let _ = (story_id, limit);
        Ok(Vec::new())
    }

    async fn commit_turn(&self, commit: &TurnCommit) -> Result<(), AiseError> {
        // Framework stub: wrap all writes in one transaction.
        let _ = commit;
        Ok(())
    }
}
