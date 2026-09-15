use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryHistoryConfig {
    pub default_page_size: usize,
    pub max_page_size: usize,
    pub max_player_contribution_bytes: usize,
    pub max_story_text_bytes: usize,
}

impl Default for StoryHistoryConfig {
    fn default() -> Self {
        Self {
            default_page_size: 20,
            max_page_size: 100,
            max_player_contribution_bytes: 16 * 1024,
            max_story_text_bytes: 64 * 1024,
        }
    }
}

impl StoryHistoryConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.default_page_size == 0
            || self.max_page_size == 0
            || self.max_player_contribution_bytes == 0
            || self.max_story_text_bytes == 0
            || self.default_page_size > self.max_page_size
        {
            return Err("story history limits are invalid");
        }
        Ok(())
    }
}
