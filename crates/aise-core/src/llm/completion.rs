use crate::core::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct LlmCompletionSpec {
    pub messages: Vec<ChatMessage>,
}

#[derive(Clone, Debug)]
pub struct LlmCompletionResult {
    pub content: String,
}

impl From<LlmCompletionResponse> for LlmCompletionResult {
    fn from(response: LlmCompletionResponse) -> Self {
        Self {
            content: response.content,
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmCompletionFinishReason {
    Stop,
    Timeout,
    Length,
    ContentFilter,
    Other,
}

#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmReasoningEffort {
    #[default]
    None,
    Low,
    Medium,
    High,
    Max,
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub reasoning_effort: Option<LlmReasoningEffort>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LlmTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmCompletionResponse {
    pub content: String,
    pub finish_reason: LlmCompletionFinishReason,
    pub usage: Option<LlmTokenUsage>,
}
