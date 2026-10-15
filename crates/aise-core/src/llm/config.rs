use serde::Deserialize;
use std::fmt::{Debug, Error, Formatter};
use std::num::NonZeroUsize;

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
    pub max_concurrent_calls: NonZeroUsize,
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
            max_concurrent_calls: NonZeroUsize::new(16).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

impl Debug for LlmConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "LlmConfig {{ provider: {:?}, base_url: {}, model: {}, temperature: {:.2}, timeout_ms: {}, max_concurrent_calls: {} }}",
            self.provider, self.base_url, self.model, self.temperature, self.timeout_ms, self.max_concurrent_calls
        )
    }
}
