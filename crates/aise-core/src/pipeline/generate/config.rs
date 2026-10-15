use serde::Deserialize;

const DEFAULT_MAX_ROLE_BYTES_PER_ROLE: usize = 16 * 1024;
const DEFAULT_MAX_ROLE_CONTEXT_BYTES: usize = 384 * 1024;
const DEFAULT_MAX_DECISION_CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateConfig {
    pub max_role_bytes_per_role: usize,
    pub max_role_context_bytes: usize,
    pub max_decision_context_bytes: usize,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            max_role_bytes_per_role: DEFAULT_MAX_ROLE_BYTES_PER_ROLE,
            max_role_context_bytes: DEFAULT_MAX_ROLE_CONTEXT_BYTES,
            max_decision_context_bytes: DEFAULT_MAX_DECISION_CONTEXT_BYTES,
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
