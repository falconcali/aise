use serde::Deserialize;

const DEFAULT_MAX_RECENT_TURNS: usize = 4096;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StoryStoreType {
    Memory,
    Sqlite,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStoreConfig {
    pub store_type: StoryStoreType,
    pub max_recent_turns: usize,
}

impl Default for StoryStoreConfig {
    fn default() -> Self {
        Self {
            store_type: StoryStoreType::Memory,
            max_recent_turns: DEFAULT_MAX_RECENT_TURNS,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistanceConfig {
    pub story_store: StoryStoreConfig,
}

impl Default for PersistanceConfig {
    fn default() -> Self {
        Self {
            story_store: StoryStoreConfig::default(),
        }
    }
}
