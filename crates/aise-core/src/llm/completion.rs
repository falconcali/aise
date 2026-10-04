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

#[derive(Clone, Debug, Serialize)]
pub struct LlmCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmCompletionResponse {
    pub content: String,
    pub finish_reason: LlmCompletionFinishReason,
}
