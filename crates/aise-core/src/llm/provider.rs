use crate::llm::{LlmCompletionRequest, LlmCompletionResponse, LlmError};
use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError>;
}
