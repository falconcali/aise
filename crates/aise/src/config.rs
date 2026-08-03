use serde::{Deserialize, Serialize};

/// Root engine config. Runtime state never hangs off config types
/// (R-CODE-06). All fields are optional when deserializing from TOML so a
/// config file may override only what it needs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiseConfig {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub turn: TurnConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// OpenAI-compatible endpoint root, e.g. `https://api.openai.com/v1`.
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: String,
    /// Shared concurrency budget for all LLM calls (R-CONC-04).
    #[serde(default)]
    pub max_concurrent: usize,
    #[serde(default)]
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// SQLite file path, e.g. `data/aise.db` or `:memory:`.
    #[serde(default)]
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnConfig {
    #[serde(default)]
    pub max_repair_rounds: u32,
    #[serde(default)]
    pub max_tokens: u32,
    #[serde(default)]
    pub max_retrieved_items: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            api_key: None,
            model: "qwen2.5".into(),
            max_concurrent: 4,
            temperature: 0.8,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_url: "data/aise.db".into(),
        }
    }
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 3,
            max_tokens: 2048,
            max_retrieved_items: 20,
        }
    }
}
