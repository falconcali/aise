use async_trait::async_trait;

use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use crate::error::AiseError;

/// Storage boundary of the engine. Implementations own transactionality
/// (R-AISE-05): a single `commit_turn` call must be atomic, consistent, and
/// recoverable.
#[async_trait]
pub trait Store: Send + Sync {
    async fn load_world(&self, story_id: &StoryId) -> Result<Option<WorldState>, AiseError>;
    async fn load_characters(&self, story_id: &StoryId) -> Result<Vec<CharacterState>, AiseError>;
    async fn load_memory(&self, character_id: &CharacterId, limit: usize) -> Result<Vec<MemoryEntry>, AiseError>;
    async fn load_story(&self, story_id: &StoryId, limit: usize) -> Result<Vec<StoryTurn>, AiseError>;

    /// Atomically commits one Turn: story turn, events, character/world/memory
    /// updates, and summary.
    async fn commit_turn(&self, commit: &TurnCommit) -> Result<(), AiseError>;
}

/// Everything produced by one committed Turn (Architecture.md §14).
#[derive(Debug, Clone)]
pub struct TurnCommit {
    pub story_id: StoryId,
    pub turn: StoryTurn,
    pub events: Vec<crate::domain::narrative::StoryEvent>,
    pub characters: Vec<CharacterState>,
    pub world: Option<WorldState>,
    pub memory: Vec<MemoryEntry>,
    pub summary: String,
}
