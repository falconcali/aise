use crate::llm::error::LlmError;
use crate::llm::message::CompletionRequest;
use async_trait::async_trait;

pub type DeltaSink = Box<dyn FnMut(String) + Send>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: &CompletionRequest) -> Result<String, LlmError>;

    async fn complete_stream(&self, req: &CompletionRequest, on_delta: DeltaSink) -> Result<(), LlmError>;
}
