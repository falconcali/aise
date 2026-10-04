use serde::{Deserialize, Serialize};
use crate::llm::{LlmCompletionSpec, Message};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptSpec {
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptResult {
    pub messages: Vec<Message>,
}