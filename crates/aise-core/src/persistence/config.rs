use crate::core::AssetConfig;
use serde::Deserialize;

const DEFAULT_MAX_RECENT_TURNS: usize = 4096;

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
    #[serde(default)]
    pub asset: AssetConfig,
}

impl Default for PackStoreConfig {
    fn default() -> Self {
        Self {
            store_type: StoreType::Memory,
            asset: AssetConfig::default(),
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
