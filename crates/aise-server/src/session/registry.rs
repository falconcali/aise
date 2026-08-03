use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aise::domain::{StoryId, TurnId};
use uuid::Uuid;

use super::model::{Session, SessionId, SessionInfo};

/// Owns the session lifecycle: create/list/delete, per-session Turn
/// serialization, and a bounded quota with eviction (R-ARCH-04). One owner
/// (R-ARCH-02): the registry, nothing else, mutates the map.
pub struct SessionRegistry {
    sessions: tokio::sync::Mutex<HashMap<SessionId, Arc<Session>>>,
    capacity: usize,
}

impl SessionRegistry {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            sessions: tokio::sync::Mutex::new(HashMap::new()),
            capacity,
        })
    }

    pub async fn create(&self, name: String) -> Result<Arc<Session>, super::SessionError> {
        let mut map = self.sessions.lock().await;
        if map.len() >= self.capacity {
            return Err(super::SessionError::QuotaExceeded(self.capacity));
        }
        let id = SessionId::new(Uuid::new_v4().to_string());
        // Story id is independent from session id; the engine only sees stories.
        let story_id = StoryId::from(Uuid::new_v4().to_string());
        let created_at = now_millis();
        let session = Session::new(id, name, story_id, created_at);
        map.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn get(&self, id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.lock().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        let map = self.sessions.lock().await;
        let mut items: Vec<SessionInfo> = map.values().map(|s| s.info()).collect();
        items.sort_by_key(|s| s.created_at);
        items
    }

    /// Removes the session and returns whether it existed.
    pub async fn delete(&self, id: &SessionId) -> bool {
        self.sessions.lock().await.remove(id).is_some()
    }

    pub async fn new_turn_id(&self) -> TurnId {
        TurnId::from(Uuid::new_v4().to_string())
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
}
