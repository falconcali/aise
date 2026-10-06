use serde::Deserialize;
use std::fmt::{Debug, Error, Formatter};

#[derive(Clone, Debug, Deserialize)]
pub enum LlmProviderType {
    OpenAiCompat,
    Other,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmConfig {
    pub provider: LlmProviderType,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub timeout_ms: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProviderType::OpenAiCompat,
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            temperature: 0.5,
            timeout_ms: 10000,
        }
    }
}

impl Debug for LlmConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "LlmConfig {{ provider: {:?}, base_url: {}, model: {}, temperature: {:.2}, timeout_ms: {} }}",
            self.provider, self.base_url, self.model, self.temperature, self.timeout_ms
        )
    }
}
