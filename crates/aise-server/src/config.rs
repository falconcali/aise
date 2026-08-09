use crate::tasks::TurnTaskSupervisorConfig;
use aise::AiseConfig;
use aise::config::ConfigError as AiseConfigError;
use aise::config::ThinkingMode;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use thiserror::Error;

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

    #[serde(default = "default_admission_capacity")]
    pub admission_capacity: usize,

    #[serde(default = "default_admission_timeout_ms")]
    pub admission_timeout_ms: u64,

    #[serde(default = "default_shutdown_grace_ms")]
    pub shutdown_grace_ms: u64,

    #[serde(default = "default_trace_dir")]
    pub trace_dir: PathBuf,

    #[serde(default = "default_trace_channel_capacity")]
    pub trace_channel_capacity: usize,
    #[serde(default = "default_trace_max_record_bytes")]
    pub trace_max_record_bytes: usize,
    #[serde(default = "default_trace_rotation_bytes")]
    pub trace_rotation_bytes: u64,
    #[serde(default = "default_trace_retention_files")]
    pub trace_retention_files: usize,
    #[serde(default = "default_trace_shutdown_grace_ms")]
    pub trace_shutdown_grace_ms: u64,

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

fn default_admission_capacity() -> usize {
    64
}

fn default_admission_timeout_ms() -> u64 {
    10_000
}

fn default_shutdown_grace_ms() -> u64 {
    10_000
}

fn default_trace_dir() -> PathBuf {
    PathBuf::from("trace")
}

fn default_trace_channel_capacity() -> usize {
    256
}

fn default_trace_max_record_bytes() -> usize {
    128 * 1024
}

fn default_trace_rotation_bytes() -> u64 {
    64 * 1024 * 1024
}

fn default_trace_retention_files() -> usize {
    16
}

fn default_trace_shutdown_grace_ms() -> u64 {
    5_000
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
            admission_capacity: default_admission_capacity(),
            admission_timeout_ms: default_admission_timeout_ms(),
            shutdown_grace_ms: default_shutdown_grace_ms(),
            trace_dir: default_trace_dir(),
            trace_channel_capacity: default_trace_channel_capacity(),
            trace_max_record_bytes: default_trace_max_record_bytes(),
            trace_rotation_bytes: default_trace_rotation_bytes(),
            trace_retention_files: default_trace_retention_files(),
            trace_shutdown_grace_ms: default_trace_shutdown_grace_ms(),
            aise: AiseConfig::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid config file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("invalid environment override {env}: {message}")]
    Env { env: &'static str, message: String },
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

impl From<AiseConfigError> for ConfigError {
    fn from(error: AiseConfigError) -> Self {
        ConfigError::Invalid(error.to_string())
    }
}

impl From<crate::tasks::TurnTaskError> for ConfigError {
    fn from(error: crate::tasks::TurnTaskError) -> Self {
        ConfigError::Invalid(error.to_string())
    }
}

impl ServerConfig {
    pub fn trace_writer_config(&self) -> crate::trace::TraceWriterConfig {
        crate::trace::TraceWriterConfig {
            channel_capacity: self.trace_channel_capacity,
            max_record_bytes: self.trace_max_record_bytes,
            rotation_bytes: self.trace_rotation_bytes,
            retention_files: self.trace_retention_files,
            shutdown_grace_ms: self.trace_shutdown_grace_ms,
        }
    }

    pub fn turn_tasks(&self) -> TurnTaskSupervisorConfig {
        TurnTaskSupervisorConfig {
            max_active_turns: self.max_concurrent_turns,
            admission_capacity: self.admission_capacity,
            admission_timeout_ms: self.admission_timeout_ms,
            shutdown_grace_ms: self.shutdown_grace_ms,
        }
    }

    pub fn config_path() -> PathBuf {
        std::env::var_os("AISE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("config").join("aise_config.toml"))
    }

    pub fn load() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let path = Self::config_path();
        let mut config = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<ServerConfig>(&text).map_err(|source| ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ServerConfig::default(),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        config.apply_env_overrides()?;
        config.resolve_llm_api_key();
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_sessions == 0 {
            return Err(ConfigError::Invalid("max_sessions must be positive".into()));
        }
        self.turn_tasks().validate()?;
        self.trace_writer_config()
            .validate()
            .map_err(|error| ConfigError::Invalid(error.to_string()))?;
        self.aise.validate().map_err(ConfigError::from)
    }

    pub fn resolved_assets_dir(&self) -> PathBuf {
        self.assets_dir.clone().unwrap_or_else(default_assets_dir)
    }

    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        self.apply_env_overrides_with(|name| {
            let v = std::env::var(name).ok()?;
            if v.is_empty() { None } else { Some(v) }
        })
    }

    fn apply_env_overrides_with(&mut self, get: impl Fn(&str) -> Option<String>) -> Result<(), ConfigError> {
        if let Some(v) = get("AISE_LISTEN_ADDR") {
            self.listen_addr = v.parse::<SocketAddr>().map_err(|e| ConfigError::Env {
                env: "AISE_LISTEN_ADDR",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_ASSETS_DIR") {
            self.assets_dir = Some(PathBuf::from(v));
        }
        if let Some(v) = get("AISE_MAX_SESSIONS") {
            self.max_sessions = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_MAX_SESSIONS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_MAX_CONCURRENT_TURNS") {
            self.max_concurrent_turns = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_MAX_CONCURRENT_TURNS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_ADMISSION_CAPACITY") {
            self.admission_capacity = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_ADMISSION_CAPACITY",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_ADMISSION_TIMEOUT_MS") {
            self.admission_timeout_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_ADMISSION_TIMEOUT_MS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_SHUTDOWN_GRACE_MS") {
            self.shutdown_grace_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_SHUTDOWN_GRACE_MS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TRACE_DIR") {
            self.trace_dir = PathBuf::from(v);
        }
        if let Some(v) = get("AISE_TRACE_CHANNEL_CAPACITY") {
            self.trace_channel_capacity = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_TRACE_CHANNEL_CAPACITY",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TRACE_MAX_RECORD_BYTES") {
            self.trace_max_record_bytes = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_TRACE_MAX_RECORD_BYTES",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TRACE_ROTATION_BYTES") {
            self.trace_rotation_bytes = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TRACE_ROTATION_BYTES",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TRACE_RETENTION_FILES") {
            self.trace_retention_files = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_TRACE_RETENTION_FILES",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TRACE_SHUTDOWN_GRACE_MS") {
            self.trace_shutdown_grace_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TRACE_SHUTDOWN_GRACE_MS",
                message: e.to_string(),
            })?;
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
            self.aise.llm.max_concurrent = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_MAX_CONCURRENT",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_LLM_TEMPERATURE") {
            self.aise.llm.temperature = v.parse::<f32>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_TEMPERATURE",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_LLM_QUEUE_TIMEOUT_MS") {
            self.aise.llm.queue_timeout_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_QUEUE_TIMEOUT_MS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_LLM_PROVIDER_TIMEOUT_MS") {
            self.aise.llm.provider_timeout_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_PROVIDER_TIMEOUT_MS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_LLM_REQUESTS_PER_MINUTE") {
            let parsed: u32 = v.parse::<u32>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_REQUESTS_PER_MINUTE",
                message: e.to_string(),
            })?;
            self.aise.llm.requests_per_minute =
                Some(std::num::NonZeroU32::new(parsed).ok_or_else(|| ConfigError::Env {
                    env: "AISE_LLM_REQUESTS_PER_MINUTE",
                    message: "must be positive".into(),
                })?);
        }
        if let Some(v) = get("AISE_LLM_TOKENS_PER_MINUTE") {
            let parsed: u32 = v.parse::<u32>().map_err(|e| ConfigError::Env {
                env: "AISE_LLM_TOKENS_PER_MINUTE",
                message: e.to_string(),
            })?;
            self.aise.llm.tokens_per_minute =
                Some(std::num::NonZeroU32::new(parsed).ok_or_else(|| ConfigError::Env {
                    env: "AISE_LLM_TOKENS_PER_MINUTE",
                    message: "must be positive".into(),
                })?);
        }
        if let Some(v) = get("AISE_LLM_THINKING") {
            match v.as_str() {
                "enabled" => self.aise.llm.thinking = Some(ThinkingMode::Enabled),
                "disabled" => self.aise.llm.thinking = Some(ThinkingMode::Disabled),
                other => {
                    return Err(ConfigError::Env {
                        env: "AISE_LLM_THINKING",
                        message: format!("unexpected value {other:?}"),
                    });
                }
            }
        }

        if let Some(v) = get("AISE_DB_URL") {
            self.aise.storage.database_url = v;
        }

        if let Some(v) = get("AISE_TURN_MAX_REPAIR_ROUNDS") {
            self.aise.turn.max_repair_rounds = v.parse::<u32>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_MAX_REPAIR_ROUNDS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TURN_MAX_TOKENS") {
            self.aise.turn.max_output_tokens = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_MAX_TOKENS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TURN_MAX_LLM_CALLS") {
            self.aise.turn.max_llm_calls = v.parse::<u32>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_MAX_LLM_CALLS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TURN_MAX_INPUT_TOKENS") {
            self.aise.turn.max_input_tokens = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_MAX_INPUT_TOKENS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TURN_MAX_TOTAL_TOKENS") {
            self.aise.turn.max_total_tokens = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_MAX_TOTAL_TOKENS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_RETRIEVAL_MAX_TOTAL_ITEMS") {
            self.aise.retrieval.max_total_items = v.parse::<usize>().map_err(|e| ConfigError::Env {
                env: "AISE_RETRIEVAL_MAX_TOTAL_ITEMS",
                message: e.to_string(),
            })?;
        }
        if let Some(v) = get("AISE_TURN_TIMEOUT_MS") {
            self.aise.turn.turn_timeout_ms = v.parse::<u64>().map_err(|e| ConfigError::Env {
                env: "AISE_TURN_TIMEOUT_MS",
                message: e.to_string(),
            })?;
        }
        Ok(())
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
