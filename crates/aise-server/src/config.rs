use aise::AiseConfig;
use aise::config::ThinkingMode;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,

    #[serde(default)]
    pub assets_dir: Option<PathBuf>,

    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    #[serde(default = "default_max_concurrent_turns")]
    pub max_concurrent_turns: usize,

    #[serde(default = "default_trace_dir")]
    pub trace_dir: PathBuf,
    #[serde(default)]
    pub aise: AiseConfig,
}

fn default_listen_addr() -> SocketAddr {
    "127.0.0.1:3000".parse().expect("static addr")
}

fn default_max_sessions() -> usize {
    64
}

fn default_max_concurrent_turns() -> usize {
    8
}

fn default_trace_dir() -> PathBuf {
    PathBuf::from("trace")
}

fn default_assets_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            assets_dir: None,
            max_sessions: default_max_sessions(),
            max_concurrent_turns: default_max_concurrent_turns(),
            trace_dir: default_trace_dir(),
            aise: AiseConfig::default(),
        }
    }
}

impl ServerConfig {
    pub fn config_path() -> PathBuf {
        std::env::var_os("AISE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("config").join("aise_config.toml"))
    }

    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

        let path = Self::config_path();
        let mut config = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<ServerConfig>(&text) {
                Ok(c) => {
                    tracing::info!(path = %path.display(), "loaded config");
                    c
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "invalid config; using defaults");
                    ServerConfig::default()
                }
            },
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "no config file; using defaults");
                ServerConfig::default()
            }
        };
        config.apply_env_overrides();
        config.resolve_llm_api_key();
        config
    }

    pub fn resolved_assets_dir(&self) -> PathBuf {
        self.assets_dir.clone().unwrap_or_else(default_assets_dir)
    }

    fn apply_env_overrides(&mut self) {
        self.apply_env_overrides_with(|name| {
            let v = std::env::var(name).ok()?;
            if v.is_empty() { None } else { Some(v) }
        });
    }

    fn apply_env_overrides_with(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(v) = get("AISE_LISTEN_ADDR") {
            match v.parse() {
                Ok(addr) => self.listen_addr = addr,
                Err(e) => {
                    tracing::warn!(env = "AISE_LISTEN_ADDR", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_ASSETS_DIR") {
            self.assets_dir = Some(PathBuf::from(v));
        }
        if let Some(v) = get("AISE_MAX_SESSIONS") {
            match v.parse() {
                Ok(n) => self.max_sessions = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_MAX_SESSIONS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_MAX_CONCURRENT_TURNS") {
            match v.parse() {
                Ok(n) => self.max_concurrent_turns = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_MAX_CONCURRENT_TURNS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TRACE_DIR") {
            self.trace_dir = PathBuf::from(v);
        }

        if let Some(v) = get("AISE_LLM_BASE_URL") {
            self.aise.llm.base_url = v;
        }
        if let Some(v) = get("AISE_LLM_API_KEY") {
            self.aise.llm.api_key = Some(v);
        }
        if let Some(v) = get("AISE_LLM_MODEL") {
            self.aise.llm.model = v;
        }
        if let Some(v) = get("AISE_LLM_MAX_CONCURRENT") {
            match v.parse() {
                Ok(n) => self.aise.llm.max_concurrent = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_MAX_CONCURRENT", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_TEMPERATURE") {
            match v.parse() {
                Ok(f) => self.aise.llm.temperature = f,
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_TEMPERATURE", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_QUEUE_TIMEOUT_MS") {
            match v.parse() {
                Ok(n) => self.aise.llm.queue_timeout_ms = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_QUEUE_TIMEOUT_MS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_PROVIDER_TIMEOUT_MS") {
            match v.parse() {
                Ok(n) => self.aise.llm.provider_timeout_ms = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_PROVIDER_TIMEOUT_MS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_REQUESTS_PER_MINUTE") {
            match v.parse() {
                Ok(n) => self.aise.llm.requests_per_minute = std::num::NonZeroU32::new(n),
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_REQUESTS_PER_MINUTE", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_TOKENS_PER_MINUTE") {
            match v.parse() {
                Ok(n) => self.aise.llm.tokens_per_minute = std::num::NonZeroU32::new(n),
                Err(e) => {
                    tracing::warn!(env = "AISE_LLM_TOKENS_PER_MINUTE", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_LLM_THINKING") {
            match v.as_str() {
                "enabled" => self.aise.llm.thinking = Some(ThinkingMode::Enabled),
                "disabled" => self.aise.llm.thinking = Some(ThinkingMode::Disabled),
                other => tracing::warn!(env = "AISE_LLM_THINKING", value = other, "ignoring invalid env override"),
            }
        }

        if let Some(v) = get("AISE_DB_URL") {
            self.aise.storage.database_url = v;
        }

        if let Some(v) = get("AISE_TURN_MAX_REPAIR_ROUNDS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_repair_rounds = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_REPAIR_ROUNDS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_TOKENS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_output_tokens = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_TOKENS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_LLM_CALLS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_llm_calls = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_LLM_CALLS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_INPUT_TOKENS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_input_tokens = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_INPUT_TOKENS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_TOTAL_TOKENS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_total_tokens = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_TOTAL_TOKENS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_RETRIEVED_ITEMS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_retrieved_items = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_MAX_RETRIEVED_ITEMS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
        if let Some(v) = get("AISE_TURN_TIMEOUT_MS") {
            match v.parse() {
                Ok(n) => self.aise.turn.turn_timeout_ms = n,
                Err(e) => {
                    tracing::warn!(env = "AISE_TURN_TIMEOUT_MS", value = %v, error = %e, "ignoring invalid env override")
                }
            }
        }
    }

    fn resolve_llm_api_key(&mut self) {
        if let Some(value) = self.aise.llm.api_key.clone() {
            self.aise.llm.api_key = resolve_api_key(value, |name| std::env::var(name).ok());
        }
    }
}

fn resolve_api_key(value: String, get_env: impl Fn(&str) -> Option<String>) -> Option<String> {
    if let Some(name) = value.strip_prefix("env:") {
        get_env(name).filter(|v| !v.is_empty())
    } else {
        match get_env(&value) {
            Some(v) if !v.is_empty() => Some(v),
            _ => Some(value),
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
