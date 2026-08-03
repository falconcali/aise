use async_trait::async_trait;

use crate::llm::error::LlmError;
use crate::llm::message::CompletionRequest;

/// A text-delta callback fed by streaming completions. Kept synchronous so
/// providers can pump deltas without owning an async channel.
pub type DeltaSink = Box<dyn FnMut(String) + Send>;

/// LLM boundary. Implementations must acquire a shared `LlmLimiter` permit
/// before every call (R-CONC-04) and be wrapped in `tracing` spans
/// (R-OBS-02).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: &CompletionRequest) -> Result<String, LlmError>;

    async fn complete_stream(&self, req: &CompletionRequest, on_delta: DeltaSink) -> Result<(), LlmError>;
}
