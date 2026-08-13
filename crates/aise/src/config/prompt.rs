use super::error::ConfigError;
use serde::{Deserialize, Serialize};
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
}

impl Default for PromptModuleConfig {
    fn default() -> Self {
        Self {
            source: PromptCatalogSourceConfig::Packaged,
        }
    }
}

impl PromptModuleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if matches!(&self.source, PromptCatalogSourceConfig::Directory { path } if path.as_os_str().is_empty()) {
            return Err(ConfigError::Invalid("prompt source directory must not be empty".into()));
        }
        Ok(())
    }
}
