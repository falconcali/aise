use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::story_sequence::StorySequence;
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct StoryHistoryQuery {
    pub after_sequence: Option<StorySequence>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryTurnView {
    pub turn_number: TurnNumber,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryOpeningView {
    pub sequence: StorySequence,
    pub story_text: String,
    pub created_at: i64,
}

pub struct StoryHistoryPage {
    pub opening: Option<StoryOpeningView>,
    pub turns: Vec<StoryTurnView>,
    pub next_after_sequence: Option<StorySequence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryHistoryConfig {
    pub default_page_size: usize,
    pub max_page_size: usize,
    pub max_player_input_bytes: usize,
    pub max_story_text_bytes: usize,
}

impl Default for StoryHistoryConfig {
    fn default() -> Self {
        Self {
            default_page_size: 20,
            max_page_size: 100,
            max_player_input_bytes: 16 * 1024,
            max_story_text_bytes: 64 * 1024,
        }
    }
}

impl StoryHistoryConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.default_page_size == 0
            || self.max_page_size == 0
            || self.max_player_input_bytes == 0
            || self.max_story_text_bytes == 0
            || self.default_page_size > self.max_page_size
        {
            return Err("story history limits are invalid");
        }
        Ok(())
    }
}

#[async_trait]
pub trait StoryHistoryReadPort: Send + Sync {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError>;
}
