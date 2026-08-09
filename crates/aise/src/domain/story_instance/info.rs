use crate::domain::ids::{StoryId, StoryRevision};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInfo {
    pub story_id: StoryId,
    pub created_at_ms: i64,
    pub base_revision: StoryRevision,
}
