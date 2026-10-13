use crate::llm::LlmConfig;
use crate::persistence::PersistanceConfig;
use crate::pipeline::PipelineConfig;
use crate::prompt::PromptConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub llm_config: LlmConfig,
    pub prompt_config: PromptConfig,
    pub persistence_config: PersistanceConfig,
    pub pipeline_config: PipelineConfig,
}
