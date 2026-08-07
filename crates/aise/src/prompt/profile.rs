use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptProfile {
    WriterPlanner,
    CharacterThink,
    StoryGenerator,
    StoryRepairer,
    NarrativeValidator,
}

impl PromptProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            PromptProfile::WriterPlanner => "writer_planner",
            PromptProfile::CharacterThink => "character_think",
            PromptProfile::StoryGenerator => "story_generator",
            PromptProfile::StoryRepairer => "story_repairer",
            PromptProfile::NarrativeValidator => "narrative_validator",
        }
    }
}

impl std::fmt::Display for PromptProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedSystemPrompt(String);

impl TrustedSystemPrompt {
    pub fn try_new(value: impl Into<String>) -> Result<Self, crate::prompt::error::PromptError> {
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedContextMessage(String);

impl UntrustedContextMessage {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
