use aise::AiseConfig;
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
    pub fn config_path() -> PathBuf {
        std::env::var_os("AISE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("config").join("server.toml"))
    }

    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

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

    pub fn resolved_assets_dir(&self) -> PathBuf {
        self.assets_dir.clone().unwrap_or_else(default_assets_dir)
    }

    fn apply_env_overrides(&mut self) {
        fn get(name: &str) -> Option<String> {
            let v = std::env::var(name).ok()?;
            if v.is_empty() { None } else { Some(v) }
        }

        if let Some(v) = get("AISE_LISTEN_ADDR") {
            match v.parse() {
                Ok(addr) => self.listen_addr = addr,
                Err(e) => eprintln!("[aise-server] ignoring AISE_LISTEN_ADDR={v}: {e}"),
            }
        }
        if let Some(v) = get("AISE_ASSETS_DIR") {
            self.assets_dir = Some(PathBuf::from(v));
        }
        if let Some(v) = get("AISE_MAX_SESSIONS") {
            match v.parse() {
                Ok(n) => self.max_sessions = n,
                Err(e) => eprintln!("[aise-server] ignoring AISE_MAX_SESSIONS={v}: {e}"),
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
                Err(e) => eprintln!("[aise-server] ignoring AISE_LLM_MAX_CONCURRENT={v}: {e}"),
            }
        }
        if let Some(v) = get("AISE_LLM_TEMPERATURE") {
            match v.parse() {
                Ok(f) => self.aise.llm.temperature = f,
                Err(e) => eprintln!("[aise-server] ignoring AISE_LLM_TEMPERATURE={v}: {e}"),
            }
        }

        if let Some(v) = get("AISE_DB_URL") {
            self.aise.storage.database_url = v;
        }

        if let Some(v) = get("AISE_TURN_MAX_REPAIR_ROUNDS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_repair_rounds = n,
                Err(e) => eprintln!("[aise-server] ignoring AISE_TURN_MAX_REPAIR_ROUNDS={v}: {e}"),
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_TOKENS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_tokens = n,
                Err(e) => eprintln!("[aise-server] ignoring AISE_TURN_MAX_TOKENS={v}: {e}"),
            }
        }
        if let Some(v) = get("AISE_TURN_MAX_RETRIEVED_ITEMS") {
            match v.parse() {
                Ok(n) => self.aise.turn.max_retrieved_items = n,
                Err(e) => eprintln!("[aise-server] ignoring AISE_TURN_MAX_RETRIEVED_ITEMS={v}: {e}"),
            }
        }
    }
}
