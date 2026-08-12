use super::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PromptCatalogSourceConfig {
    #[default]
    Packaged,
    Directory {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptModuleConfig {
    #[serde(default)]
    pub source: PromptCatalogSourceConfig,
    pub profile_assets: BTreeMap<crate::prompt::PromptProfile, crate::prompt::AssetRef>,
}

impl Default for PromptModuleConfig {
    fn default() -> Self {
        Self {
            source: PromptCatalogSourceConfig::Packaged,
            profile_assets: BTreeMap::from([
                (
                    crate::prompt::PromptProfile::WriterPlanner,
                    crate::prompt::AssetRef::new("context-v2/writer-planner-csi"),
                ),
                (
                    crate::prompt::PromptProfile::CharacterThink,
                    crate::prompt::AssetRef::new("context-v2/character-think-csi"),
                ),
                (
                    crate::prompt::PromptProfile::StoryGenerator,
                    crate::prompt::AssetRef::new("context-v1/story-generator"),
                ),
                (
                    crate::prompt::PromptProfile::StoryRepairer,
                    crate::prompt::AssetRef::new("context-v1/story-repairer"),
                ),
            ]),
        }
    }
}

impl PromptModuleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if matches!(&self.source, PromptCatalogSourceConfig::Directory { path } if path.as_os_str().is_empty()) {
            return Err(ConfigError::Invalid("prompt source directory must not be empty".into()));
        }
        let expected = [
            crate::prompt::PromptProfile::WriterPlanner,
            crate::prompt::PromptProfile::CharacterThink,
            crate::prompt::PromptProfile::StoryGenerator,
            crate::prompt::PromptProfile::StoryRepairer,
        ];
        if self.profile_assets.len() != expected.len()
            || expected.iter().any(|profile| !self.profile_assets.contains_key(profile))
        {
            return Err(ConfigError::Invalid(
                "prompt.profile_assets must contain exactly the four business profiles".into(),
            ));
        }
        for (profile, asset) in &self.profile_assets {
            if asset.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "prompt.profile_assets.{} must not be empty",
                    profile.as_str()
                )));
            }
        }
        Ok(())
    }
}
