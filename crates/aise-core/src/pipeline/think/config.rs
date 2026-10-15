use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinkConfig {
    pub max_concurrency: NonZeroUsize,
    pub max_profile_bytes: usize,
    pub max_backstory_bytes: usize,
    pub max_seed_memories: usize,
    pub max_seed_memory_bytes: usize,
    pub max_rumor_bytes: usize,
    pub max_decision_bytes: usize,
    pub max_suggested_utterance_bytes: usize,
    pub max_total_output_bytes: usize,
}

impl Default for ThinkConfig {
    fn default() -> Self {
        Self {
            max_concurrency: NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN),
            max_profile_bytes: 4 * 1024,
            max_backstory_bytes: 4 * 1024,
            max_seed_memories: 8,
            max_seed_memory_bytes: 8 * 1024,
            max_rumor_bytes: 8 * 1024,
            max_decision_bytes: 512,
            max_suggested_utterance_bytes: 512,
            max_total_output_bytes: 1024,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
