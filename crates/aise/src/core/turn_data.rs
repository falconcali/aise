use crate::core::turn_contract::StoryRevision;
use crate::domain::character::CharacterState;
use crate::domain::ids::{CharacterId, StoryId};
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::StoryTurn;
use crate::domain::world::WorldState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub max_recent_turns: usize,
    pub max_memories: usize,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    player_character_id: Option<CharacterId>,
    world: Option<WorldState>,
    characters: Vec<CharacterState>,
    recent_turns: Vec<StoryTurn>,
    player_memories: Vec<MemoryEntry>,
}

impl StoryReadSnapshot {
    pub fn new(
        story_id: StoryId,
        base_revision: StoryRevision,
        player_character_id: Option<CharacterId>,
        world: Option<WorldState>,
        characters: Vec<CharacterState>,
        recent_turns: Vec<StoryTurn>,
        player_memories: Vec<MemoryEntry>,
    ) -> Self {
        Self {
            story_id,
            base_revision,
            player_character_id,
            world,
            characters,
            recent_turns,
            player_memories,
        }
    }

    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn base_revision(&self) -> StoryRevision {
        self.base_revision
    }

    pub fn player_character_id(&self) -> Option<&CharacterId> {
        self.player_character_id.as_ref()
    }

    pub fn world(&self) -> Option<&WorldState> {
        self.world.as_ref()
    }

    pub fn characters(&self) -> &[CharacterState] {
        &self.characters
    }

    pub fn recent_turns(&self) -> &[StoryTurn] {
        &self.recent_turns
    }

    pub fn player_memories(&self) -> &[MemoryEntry] {
        &self.player_memories
    }
}

#[derive(Debug, Clone, Default)]
pub struct BaselineContext {
    pub story_instructions: String,
    pub story_config: StoryConfig,
    pub player_character: Option<CharacterState>,
    pub current_scene: Option<String>,
    pub relevant_characters: Vec<CharacterState>,
    pub recent_story: Vec<String>,
    pub story_summary: String,
    pub active_constraints: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct StoryConfig {
    pub genre: String,
    pub tone: String,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub source: ContextSource,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextSource {
    CharacterMemory,
    WorldKnowledge,
    NarrativeGraph,
    HistoricalStory,
    LoreBook,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriterPlan {
    pub retrieval_requests: Vec<ContextRequest>,
    pub character_requests: Vec<CharacterId>,
    pub story_goal: StoryGoal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub query: String,
    pub sources: Vec<ContextSource>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoryGoal {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: String,
    pub emotion: String,
    pub goal: String,
    pub possible_action: String,
}
