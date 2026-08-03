use crate::domain::character::CharacterState;
use serde::{Deserialize, Serialize};

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
