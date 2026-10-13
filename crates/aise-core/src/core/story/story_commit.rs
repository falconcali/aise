use crate::core::{Change, NarrativeState, StoryId, StoryLifeCycle, StorySummary, Turn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryCommit {
    pub story_id: StoryId,
    pub turn: Turn,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}
