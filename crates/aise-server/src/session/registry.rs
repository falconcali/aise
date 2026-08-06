use super::model::{Session, SessionId, SessionInfo};
use aise::domain::StoryId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

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

    pub async fn create(&self, name: String, story_id: StoryId) -> Result<Arc<Session>, super::SessionError> {
        let mut map = self.sessions.lock().await;
        if map.len() >= self.capacity {
            return Err(super::SessionError::QuotaExceeded(self.capacity));
        }
        let id = SessionId::new(Uuid::new_v4().to_string());
        let created_at = now_millis();
        let session = Session::new(id, name, story_id, created_at);
        map.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn bind_story(&self, session_id: &SessionId, story_id: StoryId) -> bool {
        let mut map = self.sessions.lock().await;
        let rebound = map
            .get(session_id)
            .map(|session| Session::new(session.id.clone(), session.name.clone(), story_id, session.created_at));
        match rebound {
            Some(rebound) => {
                map.insert(session_id.clone(), rebound);
                true
            }
            None => false,
        }
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

    pub async fn delete(&self, id: &SessionId) -> bool {
        self.sessions.lock().await.remove(id).is_some()
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
}
