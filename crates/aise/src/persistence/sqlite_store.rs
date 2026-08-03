use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use crate::error::AiseError;
use crate::persistence::store::{Store, TurnCommit};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

pub struct SqliteStore {
    #[allow(dead_code)]
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(url: &str) -> Result<Arc<Self>, AiseError> {
        ensure_database_dir(url)?;

        let options = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
        sqlx::migrate!("./assets/mig").run(&pool).await?;
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

#[async_trait]
impl Store for SqliteStore {
    async fn load_world(&self, story_id: &StoryId) -> Result<Option<WorldState>, AiseError> {
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
        let _ = commit;
        Ok(())
    }
}
