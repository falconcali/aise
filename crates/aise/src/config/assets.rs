use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetLimitsConfig {
    pub max_key_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tags_per_item: usize,
    pub max_topics: usize,
    pub max_topic_aliases_per_topic: usize,
    pub max_entities_per_entry: usize,
    pub max_topics_per_entry: usize,
    pub max_roles: usize,
    pub max_character_assets: usize,
    pub max_world_facts: usize,
    pub max_world_rumors: usize,
    pub max_seed_memories_per_role: usize,
    pub max_relationships_per_role: usize,
    pub max_manifest_bytes: usize,
    pub max_compressed_pack_bytes: u64,
    pub max_uncompressed_pack_bytes: u64,
    pub max_compression_ratio: u32,
    pub max_asset_files: usize,
    pub max_single_asset_bytes: u64,
    pub max_validation_issues: usize,
}

impl Default for AssetLimitsConfig {
    fn default() -> Self {
        Self {
            max_key_bytes: 128,
            max_text_bytes: 32 * 1024,
            max_tags_per_item: 32,
            max_topics: 256,
            max_topic_aliases_per_topic: 16,
            max_entities_per_entry: 16,
            max_topics_per_entry: 16,
            max_roles: 32,
            max_character_assets: 64,
            max_world_facts: 512,
            max_world_rumors: 256,
            max_seed_memories_per_role: 32,
            max_relationships_per_role: 32,
            max_manifest_bytes: 512 * 1024,
            max_compressed_pack_bytes: 32 * 1024 * 1024,
            max_uncompressed_pack_bytes: 128 * 1024 * 1024,
            max_compression_ratio: 64,
            max_asset_files: 1024,
            max_single_asset_bytes: 16 * 1024 * 1024,
            max_validation_issues: 64,
        }
    }
}

impl AssetLimitsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_key_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_key_bytes must be positive".into()));
        }
        if self.max_text_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_text_bytes must be positive".into()));
        }
        if self.max_tags_per_item == 0 {
            return Err(ConfigError::Invalid("assets.max_tags_per_item must be positive".into()));
        }
        if self.max_topics == 0 {
            return Err(ConfigError::Invalid("assets.max_topics must be positive".into()));
        }
        if self.max_topic_aliases_per_topic == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_topic_aliases_per_topic must be positive".into(),
            ));
        }
        if self.max_entities_per_entry == 0 {
            return Err(ConfigError::Invalid("assets.max_entities_per_entry must be positive".into()));
        }
        if self.max_topics_per_entry == 0 {
            return Err(ConfigError::Invalid("assets.max_topics_per_entry must be positive".into()));
        }
        if self.max_roles == 0 {
            return Err(ConfigError::Invalid("assets.max_roles must be positive".into()));
        }
        if self.max_character_assets == 0 {
            return Err(ConfigError::Invalid("assets.max_character_assets must be positive".into()));
        }
        if self.max_world_facts == 0 {
            return Err(ConfigError::Invalid("assets.max_world_facts must be positive".into()));
        }
        if self.max_world_rumors == 0 {
            return Err(ConfigError::Invalid("assets.max_world_rumors must be positive".into()));
        }
        if self.max_seed_memories_per_role == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_seed_memories_per_role must be positive".into(),
            ));
        }
        if self.max_relationships_per_role == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_relationships_per_role must be positive".into(),
            ));
        }
        if self.max_manifest_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_manifest_bytes must be positive".into()));
        }
        if self.max_compressed_pack_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_compressed_pack_bytes must be positive".into()));
        }
        if self.max_uncompressed_pack_bytes == 0 {
            return Err(ConfigError::Invalid(
                "assets.max_uncompressed_pack_bytes must be positive".into(),
            ));
        }
        if self.max_uncompressed_pack_bytes < self.max_compressed_pack_bytes {
            return Err(ConfigError::Invalid(
                "assets.max_uncompressed_pack_bytes must be >= assets.max_compressed_pack_bytes".into(),
            ));
        }
        if self.max_compression_ratio == 0 {
            return Err(ConfigError::Invalid("assets.max_compression_ratio must be positive".into()));
        }
        if self.max_asset_files == 0 {
            return Err(ConfigError::Invalid("assets.max_asset_files must be positive".into()));
        }
        if self.max_single_asset_bytes == 0 {
            return Err(ConfigError::Invalid("assets.max_single_asset_bytes must be positive".into()));
        }
        if self.max_validation_issues == 0 {
            return Err(ConfigError::Invalid("assets.max_validation_issues must be positive".into()));
        }
        Ok(())
    }
}
