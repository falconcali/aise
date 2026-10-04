use crate::core::TurnControl;
use crate::llm::llm_trace;
use crate::llm::{
    LlmCompletionRequest, LlmCompletionResponse, LlmCompletionResult, LlmCompletionSpec, LlmConfig, LlmError,
    LlmProvider,
};
use crate::trace::Observation;
use std::sync::Arc;
use std::time::{Duration, Instant};
pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
}

impl LlmGateway {
    pub fn new(provider: Arc<dyn LlmProvider>, config: LlmConfig) -> Self {
        Self { provider, config }
    }

    pub async fn complete(
        &self,
        spec: LlmCompletionSpec,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<LlmCompletionResult, LlmError> {
        let req = self.build_request(spec);
        let provider_observation = llm_trace::begin_provider_call(observation, &req);
        let rsp = self.call_provider(req, control).await;
        llm_trace::finish_provider_call(provider_observation, &rsp);

        match rsp {
            Ok(rsp) => Ok(rsp.into()),
            Err(e) => Err(e),
        }
    }

    fn build_request(&self, spec: LlmCompletionSpec) -> LlmCompletionRequest {
        LlmCompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            temperature: self.config.temperature,
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
    ) -> Result<LlmCompletionResponse, LlmError> {
        let deadline = self.call_deadline(control);
        tokio::select! {
            result = self.provider.complete(req) => result,
            _ = control.cancellation().cancelled() => Err(LlmError::Cancelled),
            _ = tokio::time::sleep_until(deadline.into()) => Err(LlmError::Timeout),
        }
    }
}
