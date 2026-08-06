use crate::core::turn_contract::{LlmCharge, LlmTokenUsage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub purpose: crate::core::turn_contract::LlmCallPurpose,
}

#[derive(Debug, Clone)]
pub struct CompletionSpec {
    pub messages: Vec<ChatMessage>,
    pub max_output_tokens: u32,
    pub purpose: crate::core::turn_contract::LlmCallPurpose,
}

#[derive(Debug, Clone)]
pub struct EmbeddingRequest {
    pub model: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingOutput {
    pub vectors: Vec<Vec<f32>>,
    pub usage: Option<LlmTokenUsage>,
    pub charge: Option<LlmCharge>,
}
