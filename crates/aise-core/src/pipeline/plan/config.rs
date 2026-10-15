use serde::Deserialize;

const DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND: usize = 32;
const DEFAULT_MAX_STORY_GOAL_BYTES: usize = 512;
const DEFAULT_MAX_ROLES_FOR_GENERATION: usize = 16;
const DEFAULT_MAX_CHARACTER_THINK_REQUESTS: usize = 8;
const DEFAULT_MAX_THINKING_FOCUS_BYTES: usize = 512;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub max_retrieval_candidates_per_kind: usize,
    pub max_story_goal_bytes: usize,
    pub max_roles_for_generation: usize,
    pub max_character_think_requests: usize,
    pub max_thinking_focus_bytes: usize,
}

impl Default for PlanConfig {
    fn default() -> Self {
        Self {
            max_retrieval_candidates_per_kind: DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND,
            max_story_goal_bytes: DEFAULT_MAX_STORY_GOAL_BYTES,
            max_roles_for_generation: DEFAULT_MAX_ROLES_FOR_GENERATION,
            max_character_think_requests: DEFAULT_MAX_CHARACTER_THINK_REQUESTS,
            max_thinking_focus_bytes: DEFAULT_MAX_THINKING_FOCUS_BYTES,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
