use crate::llm::LlmConfig;
use crate::persistence::PersistanceConfig;
use crate::prompt::PromptConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub llm_config: LlmConfig,
    pub prompt_config: PromptConfig,
    pub persistence_config: PersistanceConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            llm_config: LlmConfig::default(),
            prompt_config: PromptConfig::default(),
            persistence_config: PersistanceConfig::default(),
        }
    }
}
