use serde::Deserialize;

const DEFAULT_SCAN_DEPTH: u16 = 2;
const DEFAULT_MAX_SCAN_DEPTH: u16 = 8;
const DEFAULT_MAX_RECURSION_STEPS: u16 = 3;
const DEFAULT_MAX_ACTIVATED_ENTRIES: usize = 16;
const DEFAULT_MAX_KNOWLEDGE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationConfig {
    pub scan_depth: u16,
    pub max_scan_depth: u16,
    pub max_recursion_steps: u16,
    pub max_activated_entries: usize,
    pub max_knowledge_bytes: usize,
}

impl Default for ActivationConfig {
    fn default() -> Self {
        Self {
            scan_depth: DEFAULT_SCAN_DEPTH,
            max_scan_depth: DEFAULT_MAX_SCAN_DEPTH,
            max_recursion_steps: DEFAULT_MAX_RECURSION_STEPS,
            max_activated_entries: DEFAULT_MAX_ACTIVATED_ENTRIES,
            max_knowledge_bytes: DEFAULT_MAX_KNOWLEDGE_BYTES,
        }
    }
}
