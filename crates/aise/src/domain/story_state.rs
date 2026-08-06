use crate::core::turn_contract::{StoryId, StoryRevision};
use crate::domain::character::CharacterState;
use crate::domain::ids::CharacterId;
use crate::domain::memory::MemoryEntry;
use crate::domain::narrative::{StorySummary, StoryTurn};
use crate::domain::world::WorldState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryConfig {
    pub style: Option<String>,
    pub point_of_view: Option<String>,
    pub tense: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentScene {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryConstraint {
    pub id: ConstraintId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstraintId(String);

impl ConstraintId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, crate::core::turn_contract::TurnInputError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(crate::core::turn_contract::TurnInputError::EmptyStoryId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthoritativeStoryState {
    pub story_instructions: String,
    pub story_config: StoryConfig,
    pub current_scene: CurrentScene,
    pub story_summary: StorySummary,
    pub active_constraints: Vec<StoryConstraint>,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerStoryState {
    pub player_character_id: Option<CharacterId>,
    pub player_memories: Vec<MemoryEntry>,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    authoritative: AuthoritativeStoryState,
    player: PlayerStoryState,
    world: Option<WorldState>,
    characters: Vec<CharacterState>,
    recent_turns: Vec<StoryTurn>,
}

impl StoryReadSnapshot {
    pub fn new(
        story_id: StoryId,
        base_revision: StoryRevision,
        authoritative: AuthoritativeStoryState,
        player: PlayerStoryState,
        world: Option<WorldState>,
        characters: Vec<CharacterState>,
        recent_turns: Vec<StoryTurn>,
    ) -> Self {
        Self {
            story_id,
            base_revision,
            authoritative,
            player,
            world,
            characters,
            recent_turns,
        }
    }

    pub fn story_id(&self) -> &StoryId {
        &self.story_id
    }

    pub fn base_revision(&self) -> StoryRevision {
        self.base_revision
    }

    pub fn story_instructions(&self) -> &str {
        &self.authoritative.story_instructions
    }

    pub fn story_config(&self) -> &StoryConfig {
        &self.authoritative.story_config
    }

    pub fn player_character_id(&self) -> Option<&CharacterId> {
        self.player.player_character_id.as_ref()
    }

    pub fn world(&self) -> Option<&WorldState> {
        self.world.as_ref()
    }

    pub fn current_scene(&self) -> &CurrentScene {
        &self.authoritative.current_scene
    }

    pub fn characters(&self) -> &[CharacterState] {
        &self.characters
    }

    pub fn recent_turns(&self) -> &[StoryTurn] {
        &self.recent_turns
    }

    pub fn story_summary(&self) -> &StorySummary {
        &self.authoritative.story_summary
    }

    pub fn active_constraints(&self) -> &[StoryConstraint] {
        &self.authoritative.active_constraints
    }

    pub fn player_memories(&self) -> &[MemoryEntry] {
        &self.player.player_memories
    }
}

#[derive(Debug, Clone)]
pub struct StoryCreateSpec {
    pub story_id: StoryId,
    pub story_instructions: String,
    pub story_config: StoryConfig,
    pub player_character_id: Option<CharacterId>,
    pub initial_world: Option<WorldState>,
    pub current_scene: CurrentScene,
    pub story_summary: StorySummary,
    pub active_constraints: Vec<StoryConstraint>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryInfo {
    pub story_id: StoryId,
    pub created_at_ms: i64,
    pub base_revision: StoryRevision,
}
