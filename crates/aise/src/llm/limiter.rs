use crate::config::LlmConfig;
use crate::llm::error::LlmError;
use crate::turn::turn_contract::TurnCancellation;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const WINDOW: Duration = Duration::from_secs(60);

pub struct LlmLimiter {
    permits: Arc<Semaphore>,
    queue_timeout: Duration,
    rpm: Option<RateGate>,
    tpm: Option<RateGate>,
}

struct RateGate {
    limit: u64,
    window: Mutex<VecDeque<(Instant, u64)>>,
}

impl LlmLimiter {
    pub fn new(config: &LlmConfig) -> Result<Self, TurnExecutionError> {
        config.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_llm_config", None, error.to_string())
        })?;
        Ok(Self {
            permits: Arc::new(Semaphore::new(config.max_concurrent)),
            queue_timeout: Duration::from_millis(config.queue_timeout_ms),
            rpm: config.requests_per_minute.map(RateGate::new),
            tpm: config.tokens_per_minute.map(RateGate::new),
        })
    }

    pub async fn acquire_quota(
        &self,
        estimated_input_tokens: u64,
        max_output_tokens: u64,
        deadline: Instant,
        cancellation: &TurnCancellation,
    ) -> Result<(), LlmError> {
        if let Some(gate) = &self.rpm {
            gate.acquire(1, deadline, cancellation).await?;
        }
        if let Some(gate) = &self.tpm {
            gate.acquire(estimated_input_tokens.saturating_add(max_output_tokens), deadline, cancellation)
                .await?;
        }
        Ok(())
    }

    pub async fn acquire_permit(
        &self,
        turn_deadline: Instant,
        cancellation: &TurnCancellation,
    ) -> Result<OwnedSemaphorePermit, LlmError> {
        if cancellation.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        let now = Instant::now();
        if now >= turn_deadline {
            return Err(LlmError::TurnDeadlineExceeded);
        }
        let queue_end = now + self.queue_timeout;
        let wait_until = turn_deadline.min(queue_end);
        tokio::select! {
            permit = self.permits.clone().acquire_owned() => {
                permit.map_err(|_| LlmError::Protocol { kind: crate::llm::error::LlmProtocolErrorKind::InvalidSseLine })
            }
            _ = cancellation.token().cancelled() => Err(LlmError::Cancelled),
            _ = tokio::time::sleep_until(wait_until.into()) => {
                if Instant::now() >= turn_deadline {
                    Err(LlmError::TurnDeadlineExceeded)
                } else {
                    Err(LlmError::QueueTimeout)
                }
            }
        }
    }
}

impl RateGate {
    fn new(limit: NonZeroU32) -> Self {
        Self {
            limit: u64::from(limit.get()),
            window: Mutex::new(VecDeque::new()),
        }
    }

    async fn acquire(&self, tokens: u64, deadline: Instant, cancellation: &TurnCancellation) -> Result<(), LlmError> {
        loop {
            let wait = {
                let mut w = self.window.lock().unwrap();
                let now = Instant::now();
                while w.front().is_some_and(|(at, _)| now.duration_since(*at) >= WINDOW) {
                    w.pop_front();
                }
                let used: u64 = w.iter().map(|(_, t)| t).sum();
                if used.saturating_add(tokens) <= self.limit {
                    w.push_back((now, tokens));
                    None
                } else {
                    w.front().map(|(at, _)| (*at + WINDOW).saturating_duration_since(now))
                }
            };
            match wait {
                None => return Ok(()),
                Some(wait) => {
                    if Instant::now() >= deadline {
                        return Err(LlmError::TurnDeadlineExceeded);
                    }
                    tokio::select! {
                        _ = cancellation.token().cancelled() => return Err(LlmError::Cancelled),
                        _ = tokio::time::sleep_until(deadline.min(Instant::now() + wait).into()) => {
                            if Instant::now() >= deadline {
                                return Err(LlmError::TurnDeadlineExceeded);
                            }
                        }
                    }
                }
            }
        }
    }
}
