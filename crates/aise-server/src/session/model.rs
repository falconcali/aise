use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Session identifier; distinct from story/turn IDs (R-CODE-04).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub(crate) String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Read snapshot of a session for API responses (R-CODE-03 `XxxInfo` suffix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub story_id: String,
    pub created_at: i64,
}

/// A browser session: an HTTP resource owning one story. Sessions live here
/// (server layer); stories live in the engine.
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub story_id: aise::domain::StoryId,
    pub created_at: i64,
    /// Serializes Turns of this session: two turns for the same story must not
    /// run concurrently, or world state would race.
    turn_lock: tokio::sync::Mutex<()>,
}

impl Session {
    pub fn new(id: SessionId, name: String, story_id: aise::domain::StoryId, created_at: i64) -> Arc<Self> {
        Arc::new(Self {
            id,
            name,
            story_id,
            created_at,
            turn_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Guards the next Turn against concurrent Turns of the same session.
    pub async fn lock_turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.turn_lock.lock().await
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.to_string(),
            name: self.name.clone(),
            story_id: self.story_id.to_string(),
            created_at: self.created_at,
        }
    }
}
