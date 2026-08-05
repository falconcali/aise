use crate::config::{LlmConfig, TraceContent};
use crate::core::turn_context::TurnLlmCallScope;
use crate::core::turn_trace::{
    LlmCallContent, LlmCallData, MAX_LLM_CONTENT_CHARS, MAX_LLM_RESPONSE_CHARS, MessageData, SpanPayload, truncate,
};
use crate::error::AiseError;
use crate::llm::accounting::{LlmCompletion, LlmTokenUsage, TokenAccountant, UsageAccuracy, estimate_tokens};
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role};
use crate::llm::provider::{DeltaSink, LlmProvider};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::Instrument;

pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    limiter: LlmLimiter,
    config: LlmConfig,
    accountant: TokenAccountant,
}

impl LlmGateway {
    pub fn new(provider: Arc<dyn LlmProvider>, config: LlmConfig) -> Result<Self, AiseError> {
        config.validate()?;
        let limiter = LlmLimiter::new(&config)?;
        let accountant = TokenAccountant::new(&config, provider.provider_name());
        Ok(Self {
            provider,
            limiter,
            config,
            accountant,
        })
    }

    pub async fn complete(&self, scope: TurnLlmCallScope<'_>, spec: CompletionSpec) -> Result<LlmCompletion, LlmError> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
        };
        self.execute_call(scope, request, spec.purpose, false, None).await
    }

    pub async fn complete_stream(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        sink: DeltaSink,
    ) -> Result<LlmCompletion, LlmError> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
        };
        self.execute_call(scope, request, spec.purpose, true, Some(sink)).await
    }

    pub async fn embed(&self, scope: TurnLlmCallScope<'_>, input: String) -> Result<EmbeddingOutput, LlmError> {
        if scope.cancellation().is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        if Instant::now() >= scope.deadline() {
            return Err(LlmError::TurnDeadlineExceeded);
        }
        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            input,
        };
        self.provider.embed(&request).await
    }

    async fn execute_call(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        request: CompletionRequest,
        purpose: &'static str,
        stream: bool,
        sink: Option<DeltaSink>,
    ) -> Result<LlmCompletion, LlmError> {
        if scope.cancellation().is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        let call_started = Instant::now();
        if call_started >= scope.deadline() {
            return Err(LlmError::TurnDeadlineExceeded);
        }

        let estimated_input = TokenAccountant::estimate_input_tokens(&request.messages);
        let max_output = u64::from(request.max_tokens);
        let reservation = scope.reserve_llm(estimated_input, max_output).map_err(budget_to_llm)?;

        self.limiter
            .acquire_quota(estimated_input, max_output, scope.deadline(), scope.cancellation())
            .await?;

        let span = scope.begin_llm_span();
        let permit = self.limiter.acquire_permit(scope.deadline(), scope.cancellation()).await?;
        let queue_wait_ms = call_started.elapsed().as_millis() as u64;

        let turn_deadline = scope.deadline();
        let provider_deadline = {
            let after_timeout = Instant::now() + Duration::from_millis(self.config.provider_timeout_ms);
            after_timeout.min(turn_deadline)
        };
        let hits_turn_deadline = provider_deadline == turn_deadline;

        let tracing_span = tracing::info_span!(
            "llm.call",
            story_id = %scope.story_id(),
            turn_id = %scope.turn_id(),
            stage = %scope.stage().as_str(),
            purpose = purpose,
            provider = %self.provider.provider_name(),
            model = %self.config.model,
        );
        let provider_outcome = match stream {
            false => {
                let call = self.provider.complete(&request);
                async {
                    tokio::select! {
                        result = call => result,
                        _ = scope.cancellation().token().cancelled() => Err(LlmError::Cancelled),
                        _ = tokio::time::sleep_until(provider_deadline.into()) => {
                            if hits_turn_deadline { Err(LlmError::TurnDeadlineExceeded) } else { Err(LlmError::ProviderTimeout) }
                        }
                    }
                }
                .instrument(tracing_span)
                .await
            }
            true => {
                let on_delta = match sink {
                    Some(sink) => sink,
                    None => return Err(LlmError::Protocol("stream requires a delta sink".into())),
                };
                let call = self.provider.complete_stream(&request, on_delta);
                async {
                    tokio::select! {
                        result = call => result,
                        _ = scope.cancellation().token().cancelled() => Err(LlmError::Cancelled),
                        _ = tokio::time::sleep_until(provider_deadline.into()) => {
                            if hits_turn_deadline { Err(LlmError::TurnDeadlineExceeded) } else { Err(LlmError::ProviderTimeout) }
                        }
                    }
                }
                .instrument(tracing_span)
                .await
            }
        };
        let total_latency_ms = call_started.elapsed().as_millis() as u64;
        let provider_latency_ms = total_latency_ms.saturating_sub(queue_wait_ms);

        let (completion, provider_error) = match provider_outcome {
            Ok(mut completion) => {
                if completion.usage.is_none() {
                    completion.usage = Some(estimated_usage(&completion.text, estimated_input));
                }
                let usage = completion.usage.as_ref().expect("usage set");
                if completion.charge.is_none() {
                    completion.charge = self.accountant.charge(usage);
                }
                (Some(completion), None)
            }
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_id = %scope.turn_id(),
                    stage = %scope.stage().as_str(),
                    purpose = purpose,
                    error_kind = error.kind(),
                    error = %error,
                    "llm call failed"
                );
                (None, Some(error))
            }
        };

        let usage = completion
            .as_ref()
            .and_then(|c| c.usage.clone())
            .unwrap_or_else(|| estimated_usage("", estimated_input));
        let settle = scope.settle_llm(usage.input_tokens, usage.output_tokens);
        let usage_accuracy = usage.accuracy.as_str().to_owned();
        let charge_value = completion
            .as_ref()
            .and_then(|c| c.charge.as_ref())
            .and_then(|c| serde_json::to_value(c).ok());
        let status = if completion.is_some() { "ok" } else { "error" };
        let error_kind = provider_error.as_ref().map(|error| error.kind().to_owned());
        let finish_reason = completion
            .as_ref()
            .and_then(|c| c.finish_reason.as_ref())
            .map(|reason| reason.as_str().to_owned());
        let content = if self.config.trace_content == TraceContent::Content {
            Some(LlmCallContent {
                messages: request
                    .messages
                    .iter()
                    .map(|message| MessageData {
                        role: role_label(message.role).to_owned(),
                        content: truncate(&message.content, MAX_LLM_CONTENT_CHARS),
                    })
                    .collect(),
                response: completion
                    .as_ref()
                    .map(|c| truncate(&c.text, MAX_LLM_RESPONSE_CHARS))
                    .unwrap_or_default(),
            })
        } else {
            None
        };
        let payload = SpanPayload::LlmCall(LlmCallData {
            provider: self.provider.provider_name().to_owned(),
            model: request.model.clone(),
            purpose: purpose.to_owned(),
            stream,
            attempt: 1,
            queue_wait_ms,
            provider_latency_ms,
            total_latency_ms,
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            output_tokens: usage.output_tokens,
            usage_accuracy,
            finish_reason,
            charge: charge_value,
            status: status.to_owned(),
            error_kind,
            content,
        });
        scope.end_llm_span(span, &payload);
        drop(permit);
        let _ = reservation;
        settle.map_err(budget_to_llm)?;
        match completion {
            Some(completion) => Ok(completion),
            None => Err(provider_error.unwrap_or_else(|| LlmError::Protocol("provider returned no result".into()))),
        }
    }
}

fn estimated_usage(text: &str, estimated_input: u64) -> LlmTokenUsage {
    let output = estimate_tokens(text);
    LlmTokenUsage {
        input_tokens: estimated_input,
        cached_input_tokens: None,
        output_tokens: output,
        total_tokens: estimated_input.saturating_add(output),
        accuracy: UsageAccuracy::Estimated,
    }
}

fn budget_to_llm(error: AiseError) -> LlmError {
    match error {
        AiseError::TokenBudgetExceeded(message) => LlmError::TokenBudgetExceeded(message),
        AiseError::Cancelled => LlmError::Cancelled,
        AiseError::TurnDeadlineExceeded => LlmError::TurnDeadlineExceeded,
        other => LlmError::Protocol(format!("turn budget error: {other}")),
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
