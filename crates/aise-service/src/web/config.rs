use aise_core::engine::EngineConfig;
use aise_core::llm::{LlmConfig, LlmProviderType};
use aise_core::pipeline::PipelineConfig;
use aise_core::prompt::PromptConfig;
use anyhow::Context;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:3000";
const DEFAULT_TURN_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_INPUT_BYTES: usize = 16 * 1024;
const DEFAULT_EVENT_BUFFER_CAPACITY: NonZeroUsize = NonZeroUsize::new(64).unwrap();
const DEFAULT_LLM_TEMPERATURE: f32 = 0.7;
const DEFAULT_LLM_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_LLM_MAX_CONCURRENT_CALLS: NonZeroUsize = NonZeroUsize::new(16).unwrap();
const DEFAULT_PROMPT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../aise-core/assets/prompts");
const DEFAULT_PROMPT_MAX_PROMPTS: usize = 16;
const DEFAULT_PROMPT_MAX_TEMPLATE_BYTES: usize = 64 * 1024;
const DEFAULT_PROMPT_MAX_TOTAL_TEMPLATE_BYTES: usize = 1024 * 1024;
const DEFAULT_STORY_PACK_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack_v1.json");
const DEFAULT_CHARACTER_CARD_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/xu_xian_card_v1.json");

pub struct WebConfig {
    pub bind_address: SocketAddr,
    pub turn_timeout: Duration,
    pub max_input_bytes: usize,
    pub event_buffer_capacity: NonZeroUsize,
    pub story_pack_path: PathBuf,
    pub character_card_path: PathBuf,
    pub engine: EngineConfig,
}

impl WebConfig {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            bind_address: parsed_env("AISE_WEB_BIND", DEFAULT_BIND_ADDRESS.parse()?)?,
            turn_timeout: Duration::from_millis(parsed_env("AISE_TURN_TIMEOUT_MS", DEFAULT_TURN_TIMEOUT_MS)?),
            max_input_bytes: parsed_env("AISE_WEB_MAX_INPUT_BYTES", DEFAULT_MAX_INPUT_BYTES)?,
            event_buffer_capacity: parsed_env("AISE_WEB_EVENT_BUFFER_CAPACITY", DEFAULT_EVENT_BUFFER_CAPACITY)?,
            story_pack_path: path_env("AISE_WEB_STORY_PACK_PATH", DEFAULT_STORY_PACK_PATH),
            character_card_path: path_env("AISE_WEB_CHARACTER_CARD_PATH", DEFAULT_CHARACTER_CARD_PATH),
            engine: EngineConfig {
                llm_config: LlmConfig {
                    provider: LlmProviderType::OpenAiCompat,
                    base_url: required_env("AISE_LLM_BASE_URL")?,
                    api_key: required_env("AISE_LLM_API_KEY")?,
                    model: required_env("AISE_LLM_MODEL")?,
                    temperature: parsed_env("AISE_LLM_TEMPERATURE", DEFAULT_LLM_TEMPERATURE)?,
                    timeout_ms: parsed_env("AISE_LLM_TIMEOUT_MS", DEFAULT_LLM_TIMEOUT_MS)?,
                    max_concurrent_calls: parsed_env(
                        "AISE_LLM_MAX_CONCURRENT_CALLS",
                        DEFAULT_LLM_MAX_CONCURRENT_CALLS,
                    )?,
                },
                prompt_config: PromptConfig {
                    directory: PathBuf::from(DEFAULT_PROMPT_DIRECTORY),
                    max_prompts: DEFAULT_PROMPT_MAX_PROMPTS,
                    max_template_bytes: DEFAULT_PROMPT_MAX_TEMPLATE_BYTES,
                    max_total_template_bytes: DEFAULT_PROMPT_MAX_TOTAL_TEMPLATE_BYTES,
                },
                persistence_config: Default::default(),
                pipeline_config: PipelineConfig::default(),
            },
        })
    }
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("missing required environment variable {name}"))
}

fn parsed_env<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .with_context(|| format!("invalid value for environment variable {name}")),
        Err(_) => Ok(default),
    }
}

fn path_env(name: &str, default: &str) -> PathBuf {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}
