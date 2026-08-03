use crate::error::AiseError;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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

    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, AiseError> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AiseError::Internal("llm limiter closed".into()))
    }
}
