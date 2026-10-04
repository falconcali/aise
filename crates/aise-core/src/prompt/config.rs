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
