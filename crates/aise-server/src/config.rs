use std::net::SocketAddr;
use std::path::PathBuf;

use aise::AiseConfig;
use serde::{Deserialize, Serialize};

/// Server transport config (R-CODE-06). The engine config nests untouched.
/// All fields are optional when deserializing from TOML; missing keys fall
/// back to `Default`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
    /// Directory hosting the static frontend; `None` resolves to the crate's
    /// compiled-in assets directory.
    #[serde(default)]
    pub assets_dir: Option<PathBuf>,
    /// Bounded session quota (R-ARCH-04).
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,
    /// Log output directory. Git-ignored.
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

fn default_trace_dir() -> PathBuf {
    PathBuf::from("trace")
}

/// Compiled-in assets dir, so `cargo run` works from any working directory.
fn default_assets_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            assets_dir: None,
            max_sessions: default_max_sessions(),
            trace_dir: default_trace_dir(),
            aise: AiseConfig::default(),
        }
    }
}

impl ServerConfig {
    /// Config file path: `AISE_CONFIG` env var, else `config/server.toml`
    /// relative to the working directory.
    pub fn config_path() -> PathBuf {
        std::env::var_os("AISE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("config").join("server.toml"))
    }

    /// Loads the config file, merging missing keys with defaults, then applies
    /// environment-variable overrides. Falls back to defaults (with a message
    /// on stderr) when the file is absent or malformed, so a missing config is
    /// not fatal during development.
    pub fn load() -> Self {
        let path = Self::config_path();
        let mut config = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<ServerConfig>(&text) {
                Ok(c) => {
                    eprintln!("[aise-server] loaded config from {}", path.display());
                    c
                }
                Err(e) => {
                    eprintln!("[aise-server] invalid config {}: {e}; using defaults", path.display());
                    ServerConfig::default()
                }
            },
            Err(e) => {
                eprintln!("[aise-server] no config at {} ({e}); using defaults", path.display());
                ServerConfig::default()
            }
        };
        config.apply_env_overrides();
        config
    }

    /// Resolved static-assets directory (config value or crate default).
    pub fn resolved_assets_dir(&self) -> PathBuf {
        self.assets_dir.clone().unwrap_or_else(default_assets_dir)
    }

    /// Applies environment-variable overrides on top of the config file.
    fn apply_env_overrides(&mut self) {
        if let Ok(key) = std::env::var("AISE_LLM_API_KEY") {
            if !key.is_empty() {
                self.aise.llm.api_key = Some(key);
            }
        }
    }
}
