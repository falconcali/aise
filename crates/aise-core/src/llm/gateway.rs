use crate::core::TurnControl;
use crate::llm::llm_trace;
use crate::llm::{
    LlmCompletionRequest, LlmCompletionResponse, LlmCompletionResult, LlmCompletionSpec, LlmConfig, LlmError,
    LlmProvider, LlmReasoningEffort,
};
use crate::trace::Observation;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, SemaphorePermit};

pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
    limiter: Semaphore,
}

impl LlmGateway {
    pub fn new(provider: Arc<dyn LlmProvider>, config: LlmConfig) -> Self {
        let limiter = Semaphore::new(config.max_concurrent_calls.get());
        Self {
            provider,
            config,
            limiter,
        }
    }

    pub async fn complete(
        &self,
        spec: LlmCompletionSpec,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<LlmCompletionResult, LlmError> {
        self.complete_request(spec, Some(LlmReasoningEffort::default()), control, observation)
            .await
    }

    pub async fn complete_with_reasoning_effort(
        &self,
        spec: LlmCompletionSpec,
        reasoning_effort: LlmReasoningEffort,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<LlmCompletionResult, LlmError> {
        self.complete_request(spec, Some(reasoning_effort), control, observation).await
    }

    async fn complete_request(
        &self,
        spec: LlmCompletionSpec,
        reasoning_effort: Option<LlmReasoningEffort>,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<LlmCompletionResult, LlmError> {
        let req = self.build_request(spec, reasoning_effort);
        let provider_observation = llm_trace::begin_provider_call(observation, &req);
        let permit = tokio::select! {
            biased;
            _ = control.cancellation().cancelled() => Err(LlmError::Cancelled),
            _ = tokio::time::sleep_until(control.deadline().into()) => Err(LlmError::Timeout),
            result = self.limiter.acquire() => result.map_err(|_| LlmError::ConcurrencyLimiterClosed),
        };
        let rsp = match permit {
            Ok(permit) => self.call_provider(req, control, permit).await,
            Err(error) => Err(error),
        };
        llm_trace::finish_provider_call(provider_observation, &rsp);

        match rsp {
            Ok(rsp) => Ok(rsp.into()),
            Err(e) => Err(e),
        }
    }

    fn build_request(
        &self,
        spec: LlmCompletionSpec,
        reasoning_effort: Option<LlmReasoningEffort>,
    ) -> LlmCompletionRequest {
        LlmCompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            temperature: self.config.temperature,
            reasoning_effort,
        }
    }

    fn call_deadline(&self, control: &TurnControl) -> Instant {
        let call_timeout = Duration::from_millis(self.config.timeout_ms);
        control.deadline().min(Instant::now() + call_timeout)
    }

    async fn call_provider(
        &self,
        req: LlmCompletionRequest,
        control: &TurnControl,
        permit: SemaphorePermit<'_>,
    ) -> Result<LlmCompletionResponse, LlmError> {
        let deadline = self.call_deadline(control);
        let result = tokio::select! {
            biased;
            _ = control.cancellation().cancelled() => Err(LlmError::Cancelled),
            _ = tokio::time::sleep_until(deadline.into()) => Err(LlmError::Timeout),
            result = self.provider.complete(req) => result,
        };
        drop(permit);
        result
    }
}

#[cfg(test)]
#[path = "tests/gateway_tests.rs"]
mod tests;
