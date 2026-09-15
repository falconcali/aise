use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::story_sequence::StorySequence;
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct StoryHistoryQuery {
    pub after_sequence: Option<StorySequence>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryTurnView {
    pub turn_number: TurnNumber,
    pub sequence: StorySequence,
    pub player_contribution: String,
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

#[async_trait]
pub trait StoryHistoryReadPort: Send + Sync {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError>;
}
