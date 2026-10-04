use crate::llm::Message;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct LlmCompletionSpec {
    pub messages: Vec<Message>,
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
    pub messages: Vec<Message>,
    pub temperature: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmCompletionResponse {
    pub content: String,
    pub finish_reason: LlmCompletionFinishReason,
}