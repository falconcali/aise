use serde::Deserialize;

const DEFAULT_MAX_RECENT_TURNS: usize = 4096;
const DEFAULT_MAX_PACKS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StoreType {
    Memory,
    Sqlite,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStoreConfig {
    pub store_type: StoreType,
    pub max_recent_turns: usize,
}

impl Default for StoryStoreConfig {
    fn default() -> Self {
        Self {
            store_type: StoreType::Memory,
            max_recent_turns: DEFAULT_MAX_RECENT_TURNS,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackStoreConfig {
    pub store_type: StoreType,
    pub max_packs: usize,
}

impl Default for PackStoreConfig {
    fn default() -> Self {
        Self {
            store_type: StoreType::Memory,
            max_packs: DEFAULT_MAX_PACKS,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistanceConfig {
    #[serde(default)]
    pub story_store: StoryStoreConfig,
    #[serde(default)]
    pub pack_store: PackStoreConfig,
}
