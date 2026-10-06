use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptConfig {
    pub directory: PathBuf,
    pub max_prompts: usize,
    pub max_template_bytes: usize,
    pub max_total_template_bytes: usize,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("prompts"),
            max_prompts: 100,
            max_template_bytes: 1024,
            max_total_template_bytes: 10240,
        }
    }
}
