use crate::config::TurnContentLimitsConfig;
use crate::domain::character::CharacterState;
use crate::domain::ids::CharacterId;
use crate::domain::story_state::StoryConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    max_story_instructions_bytes: usize,
    max_story_config_bytes: usize,
    max_scene_bytes: usize,
    max_summary_bytes: usize,
    max_constraints: usize,
    max_constraint_bytes: usize,
    max_characters: usize,
    max_character_bytes: usize,
    max_world_facts: usize,
    max_world_fact_bytes: usize,
    max_recent_turns: usize,
    max_recent_turn_bytes: usize,
    max_memories: usize,
    max_memory_bytes: usize,
}

impl SnapshotLimits {
    pub fn from_config(config: &TurnContentLimitsConfig) -> Self {
        Self {
            max_story_instructions_bytes: config.max_story_instructions_bytes,
            max_story_config_bytes: config.max_story_config_bytes,
            max_scene_bytes: config.max_scene_bytes,
            max_summary_bytes: config.max_summary_bytes,
            max_constraints: config.max_constraints,
            max_constraint_bytes: config.max_constraint_bytes,
            max_characters: config.max_characters,
            max_character_bytes: config.max_character_bytes,
            max_world_facts: config.max_world_facts,
            max_world_fact_bytes: config.max_world_fact_bytes,
            max_recent_turns: config.max_recent_turns,
            max_recent_turn_bytes: config.max_recent_turn_bytes,
            max_memories: config.max_memories,
            max_memory_bytes: config.max_memory_bytes,
        }
    }

    pub fn max_story_instructions_bytes(&self) -> usize {
        self.max_story_instructions_bytes
    }

    pub fn max_story_config_bytes(&self) -> usize {
        self.max_story_config_bytes
    }

    pub fn max_scene_bytes(&self) -> usize {
        self.max_scene_bytes
    }

    pub fn max_summary_bytes(&self) -> usize {
        self.max_summary_bytes
    }

    pub fn max_constraints(&self) -> usize {
        self.max_constraints
    }

    pub fn max_constraint_bytes(&self) -> usize {
        self.max_constraint_bytes
    }

    pub fn max_characters(&self) -> usize {
        self.max_characters
    }

    pub fn max_character_bytes(&self) -> usize {
        self.max_character_bytes
    }

    pub fn max_world_facts(&self) -> usize {
        self.max_world_facts
    }

    pub fn max_world_fact_bytes(&self) -> usize {
        self.max_world_fact_bytes
    }

    pub fn max_recent_turns(&self) -> usize {
        self.max_recent_turns
    }

    pub fn max_recent_turn_bytes(&self) -> usize {
        self.max_recent_turn_bytes
    }

    pub fn max_memories(&self) -> usize {
        self.max_memories
    }

    pub fn max_memory_bytes(&self) -> usize {
        self.max_memory_bytes
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

impl BaselineContext {
    pub fn estimate_tokens(&self) -> u64 {
        let mut chars = 0usize;
        chars = chars.saturating_add(self.story_instructions.len());
        if let Some(scene) = &self.current_scene {
            chars = chars.saturating_add(scene.len());
        }
        if let Some(config) = &self.story_config.style {
            chars = chars.saturating_add(config.len());
        }
        if let Some(config) = &self.story_config.point_of_view {
            chars = chars.saturating_add(config.len());
        }
        if let Some(config) = &self.story_config.tense {
            chars = chars.saturating_add(config.len());
        }
        for character in &self.relevant_characters {
            chars = chars.saturating_add(character.name.len());
            chars = chars.saturating_add(character.bio.len());
        }
        for turn in &self.recent_story {
            chars = chars.saturating_add(turn.len());
        }
        chars = chars.saturating_add(self.story_summary.len());
        for constraint in &self.active_constraints {
            chars = chars.saturating_add(constraint.len());
        }
        (chars as u64).saturating_add(3).checked_div(4).unwrap_or(1).max(1)
    }
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub source: ContextSource,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    CharacterMemory,
    WorldKnowledge,
    NarrativeGraph,
    HistoricalStory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub query: String,
    pub sources: Vec<ContextSource>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriterPlan {
    pub retrieval_requests: Vec<ContextRequest>,
    pub character_requests: Vec<CharacterId>,
    pub story_goal: StoryGoal,
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

#[allow(dead_code)]
pub(crate) fn _turn_data_anchor(_: &BTreeMap<String, String>, _: &CharacterId, _: &StoryConfig) {}
