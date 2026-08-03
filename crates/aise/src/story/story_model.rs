use serde::{Deserialize, Serialize};

use crate::domain::character::CharacterPatch;
use crate::domain::memory::MemoryPatch;
use crate::domain::narrative::StoryEvent;
use crate::domain::world::WorldPatch;

/// Full Turn result before validation and commit (Architecture.md §11).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryDraft {
    pub story_text: String,
    pub events: Vec<StoryEvent>,
    pub character_updates: Vec<CharacterPatch>,
    pub world_updates: Vec<WorldPatch>,
    pub memory_updates: Vec<MemoryPatch>,
}
