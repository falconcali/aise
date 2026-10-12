use serde::Deserialize;

const DEFAULT_MAX_NODES: usize = 64;
const DEFAULT_MAX_CONDITION_DEPTH: usize = 8;
const DEFAULT_MAX_SEMANTIC_CONDITIONS: usize = 16;
const DEFAULT_MAX_CONSTRAINTS_PER_NODE: usize = 8;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeConfig {
    pub max_nodes: usize,
    pub max_condition_depth: usize,
    pub max_semantic_conditions: usize,
    pub max_constraints_per_node: usize,
}

impl Default for NarrativeConfig {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES,
            max_condition_depth: DEFAULT_MAX_CONDITION_DEPTH,
            max_semantic_conditions: DEFAULT_MAX_SEMANTIC_CONDITIONS,
            max_constraints_per_node: DEFAULT_MAX_CONSTRAINTS_PER_NODE,
        }
    }
}
