use crate::core::NarrativeConfig;
use serde::Deserialize;

const DEFAULT_MAX_LABEL_BYTES: usize = 256;
const DEFAULT_MAX_TEXT_BYTES: usize = 4096;
const DEFAULT_MAX_OPENING_BYTES: usize = 16384;
const DEFAULT_MAX_LIST_ITEMS: usize = 32;
const DEFAULT_MAX_ROLES: usize = 32;
const DEFAULT_MAX_SEED_ENTRIES: usize = 256;
const DEFAULT_MAX_SEED_MEMORIES_PER_ROLE: usize = 64;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetConfig {
    pub max_label_bytes: usize,
    pub max_text_bytes: usize,
    pub max_opening_bytes: usize,
    pub max_list_items: usize,
    pub max_roles: usize,
    pub max_seed_entries: usize,
    pub max_seed_memories_per_role: usize,
    #[serde(default)]
    pub narrative: NarrativeConfig,
}

impl Default for AssetConfig {
    fn default() -> Self {
        Self {
            max_label_bytes: DEFAULT_MAX_LABEL_BYTES,
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_opening_bytes: DEFAULT_MAX_OPENING_BYTES,
            max_list_items: DEFAULT_MAX_LIST_ITEMS,
            max_roles: DEFAULT_MAX_ROLES,
            max_seed_entries: DEFAULT_MAX_SEED_ENTRIES,
            max_seed_memories_per_role: DEFAULT_MAX_SEED_MEMORIES_PER_ROLE,
            narrative: NarrativeConfig::default(),
        }
    }
}
