use crate::core::turn_validation::StateChange;
use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use crate::error::AiseError;
use async_trait::async_trait;

#[async_trait]
pub trait Store: Send + Sync {
    async fn load_world(&self, story_id: &StoryId) -> Result<Option<WorldState>, AiseError>;
    async fn load_characters(&self, story_id: &StoryId) -> Result<Vec<CharacterState>, AiseError>;
    async fn load_memory(&self, character_id: &CharacterId, limit: usize) -> Result<Vec<MemoryEntry>, AiseError>;
    async fn load_story(&self, story_id: &StoryId, limit: usize) -> Result<Vec<StoryTurn>, AiseError>;

    async fn commit_turn(&self, commit: &TurnCommit) -> Result<(), AiseError>;
}

#[derive(Debug, Clone)]
pub struct TurnCommit {
    pub story_id: StoryId,
    pub turn: StoryTurn,
    pub events: Vec<crate::domain::narrative::StoryEvent>,
    pub characters: Vec<CharacterState>,
    pub world: StateChange<WorldState>,
    pub memory: Vec<MemoryEntry>,
}
