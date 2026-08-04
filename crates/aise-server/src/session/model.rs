use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub story_id: String,
    pub created_at: i64,
}

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub story_id: aise::domain::StoryId,
    pub created_at: i64,
}

impl Session {
    pub fn new(id: SessionId, name: String, story_id: aise::domain::StoryId, created_at: i64) -> Arc<Self> {
        Arc::new(Self {
            id,
            name,
            story_id,
            created_at,
        })
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
