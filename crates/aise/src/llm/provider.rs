use crate::llm::accounting::LlmCompletion;
use crate::llm::error::LlmProviderError;
use crate::llm::message::{CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use async_trait::async_trait;

pub type DeltaSink = Box<dyn FnMut(String) + Send>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError>;

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
        on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError>;

    async fn embed(&self, req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError>;
}
