use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::AiseError;

/// Shared concurrency limiter for every LLM call (R-CONC-04). Clone is cheap;
/// hand the same limiter to all providers/pipelines.
#[derive(Clone)]
pub struct LlmLimiter {
    permits: Arc<Semaphore>,
}

impl LlmLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Acquires a permit, blocking until the shared budget frees up.
    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, AiseError> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AiseError::Internal("llm limiter closed".into()))
    }
}
