use serde::Deserialize;

const DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND: usize = 32;
const DEFAULT_MAX_STORY_GOAL_BYTES: usize = 512;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub max_retrieval_candidates_per_kind: usize,
    pub max_story_goal_bytes: usize,
}

impl Default for PlanConfig {
    fn default() -> Self {
        Self {
            max_retrieval_candidates_per_kind: DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND,
            max_story_goal_bytes: DEFAULT_MAX_STORY_GOAL_BYTES,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
