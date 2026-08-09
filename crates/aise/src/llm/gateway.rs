use crate::config::{LlmConfig, TraceContentPolicy};
use crate::core::token_estimator::estimate_text_tokens;
use crate::core::turn_context::TurnLlmCallScope;
use crate::core::turn_contract::{LlmBudgetReservation, LlmCallPurpose, LlmCallStatus, LlmCallUsage, UsageAccuracy};
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_trace::{
    LlmCallContent, LlmCallData, MAX_LLM_CONTENT_CHARS, MAX_LLM_RESPONSE_CHARS, MessageData, SpanPayload, truncate,
};
use crate::llm::accounting::{FinishReason, LlmCompletion, TokenAccountant};
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{ChatMessage, CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role};
use crate::llm::provider::{DeltaSink, LlmProvider};
use crate::prompt::{ModelRequest, RuntimeContextEncoder, TrustedPromptSource};
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::Instrument;

pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    prompt_source: Arc<dyn TrustedPromptSource>,
    limiter: LlmLimiter,
    config: LlmConfig,
    accountant: TokenAccountant,
    context_encoder: RuntimeContextEncoder,
}

impl LlmGateway {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        prompt_source: Arc<dyn TrustedPromptSource>,
        config: LlmConfig,
    ) -> Result<Self, TurnExecutionError> {
        config.validate().map_err(|error| {
            crate::core::turn_error::TurnExecutionError::new(
                crate::core::turn_error::TurnFailureKind::InvalidRequest,
                "invalid_llm_config",
                None,
                error.to_string(),
            )
        })?;
        let limiter = LlmLimiter::new(&config)?;
        let accountant = TokenAccountant::new(&config, provider.provider_name());
        Ok(Self {
            provider,
            prompt_source,
            limiter,
            config,
            accountant,
            context_encoder: RuntimeContextEncoder,
        })
    }

    pub async fn complete_typed<C: Serialize>(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        request: ModelRequest<C>,
    ) -> Result<LlmCompletion, LlmError> {
        let system_prompt = self
            .prompt_source
            .resolve(request.profile())
            .map_err(|_error| LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
            })?;
        let context_message = self
            .context_encoder
            .encode(request.context())
            .map_err(|_error| LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
            })?;
        let spec = CompletionSpec {
            messages: vec![
                ChatMessage {
                    role: Role::System,
                    content: system_prompt.as_str().to_owned(),
                },
                ChatMessage {
                    role: Role::User,
                    content: context_message.as_str().to_owned(),
                },
            ],
            max_output_tokens: request.max_output_tokens(),
            purpose: request.purpose(),
        };
        let estimated_input = crate::llm::accounting::TokenAccountant::estimate_input_tokens(&spec.messages);
        let reservation = scope
            .reserve_llm(estimated_input, u64::from(spec.max_output_tokens))
            .map_err(|error| LlmError::TokenBudgetExceeded(error.to_string()))?;
        self.complete(scope, spec, reservation).await
    }

    pub async fn complete(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
        };
        self.execute_call(&mut scope, request, false, None, reservation).await
    }

    pub async fn complete_stream(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        sink: DeltaSink,
    ) -> Result<LlmCompletion, LlmError> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
        };
        self.execute_call_owned(scope, request, true, Some(sink), reservation).await
    }

    pub async fn embed(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        inputs: Vec<String>,
        reservation: LlmBudgetReservation,
    ) -> Result<EmbeddingOutput, LlmError> {
        if scope.cancellation().is_cancelled() {
            scope.release_llm(reservation);
            return Err(LlmError::Cancelled);
        }
        if inputs.len() > self.config.protocol.max_embedding_items {
            scope.release_llm(reservation);
            return Err(LlmError::ResponseLimitExceeded {
                limit: crate::llm::error::LlmResponseLimit::EmbeddingItems,
            });
        }
        let call_started = Instant::now();
        if call_started >= scope.deadline() {
            scope.release_llm(reservation);
            return Err(LlmError::TurnDeadlineExceeded);
        }
        let estimated_input: u64 = inputs.iter().map(|input| estimate_text_tokens(input)).sum();
        let max_output = 0u64;
        if let Err(error) = self
            .limiter
            .acquire_quota(estimated_input, max_output, scope.deadline(), scope.cancellation())
            .await
        {
            scope.release_llm(reservation);
            return Err(error);
        }
        let permit = match self.limiter.acquire_permit(scope.deadline(), scope.cancellation()).await {
            Ok(permit) => permit,
            Err(error) => {
                scope.release_llm(reservation);
                return Err(error);
            }
        };
        let span = scope.begin_llm_span();
        let queue_wait_ms = call_started.elapsed().as_millis() as u64;

        let turn_deadline = scope.deadline();
        let provider_deadline = {
            let after_timeout = Instant::now() + Duration::from_millis(self.config.provider_timeout_ms);
            after_timeout.min(turn_deadline)
        };
        let hits_turn_deadline = provider_deadline == turn_deadline;

        let tracing_span = tracing::info_span!(
            "llm.embed",
            story_id = %scope.story_id(),
            turn_id = %scope.turn_id(),
            stage = %scope.stage().as_str(),
            provider = %self.provider.provider_name(),
            model = %self.config.model,
        );
        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            inputs,
        };
        let provider_outcome = {
            let call = self.provider.embed(&request);
            async {
                tokio::select! {
                    result = call => result.map_err(LlmError::from),
                    _ = scope.cancellation().token().cancelled() => Err(LlmError::Cancelled),
                    _ = tokio::time::sleep_until(provider_deadline.into()) => {
                        if hits_turn_deadline {
                            Err(LlmError::TurnDeadlineExceeded)
                        } else {
                            Err(LlmError::ProviderTimeout)
                        }
                    }
                }
            }
            .instrument(tracing_span)
            .await
        };
        let total_latency_ms = call_started.elapsed().as_millis() as u64;
        let provider_latency_ms = total_latency_ms.saturating_sub(queue_wait_ms);

        let (output, provider_error) = match provider_outcome {
            Ok(output) => (Some(output), None),
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_id = %scope.turn_id(),
                    stage = %scope.stage().as_str(),
                    error_kind = error.kind(),
                    error = %error,
                    "embedding call failed"
                );
                (None, Some(error))
            }
        };
        let usage =
            output
                .as_ref()
                .and_then(|output| output.usage.clone())
                .unwrap_or(crate::llm::accounting::LlmTokenUsage {
                    input_tokens: estimated_input,
                    cached_input_tokens: None,
                    output_tokens: 0,
                    reasoning_tokens: None,
                    total_tokens: estimated_input,
                    accuracy: UsageAccuracy::Estimated,
                });
        let charge_value = output
            .as_ref()
            .and_then(|output| output.charge.as_ref())
            .and_then(|charge| serde_json::to_value(charge).ok());
        let call_usage = self.usage_to_call_usage(LlmCallPurpose::Embedding, &usage, &charge_value, None);
        let settle = scope.settle_llm(reservation, call_usage);
        let status = if let Some(error) = &provider_error {
            llm_status_from_error(error)
        } else if settle.is_err() {
            LlmCallStatus::TokenBudgetExceeded
        } else if output.is_some() {
            LlmCallStatus::Succeeded
        } else {
            LlmCallStatus::ProviderRejected
        };
        let error_kind = provider_error.as_ref().map(|error| error.kind().to_owned());
        let payload = SpanPayload::LlmCall(Box::new(LlmCallData {
            provider: self.provider.provider_name().to_owned(),
            model: request.model.clone(),
            purpose: "embedding".into(),
            stream: false,
            attempt: 1,
            queue_wait_ms,
            provider_latency_ms,
            total_latency_ms,
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            output_tokens: 0,
            reasoning_tokens: None,
            usage_accuracy: usage.accuracy.as_str().to_owned(),
            finish_reason: None,
            charge: charge_value,
            status: status.as_str().to_owned(),
            error_kind,
            content: None,
        }));
        scope.end_llm_span(span, &payload);
        drop(permit);
        settle.map_err(budget_to_llm)?;
        match output {
            Some(output) => Ok(output),
            None => Err(provider_error.unwrap_or(LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
            })),
        }
    }

    async fn execute_call(
        &self,
        scope: &mut TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError> {
        if stream && sink.is_none() {
            return Err(LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::InvalidSseLine,
            });
        }
        self.run_call(scope, request, stream, sink, reservation).await
    }

    async fn execute_call_owned(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError> {
        self.run_call(&mut scope, request, stream, sink, reservation).await
    }

    async fn run_call(
        &self,
        scope: &mut TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError> {
        let span = scope.begin_llm_span();
        let call_started = Instant::now();

        if scope.cancellation().is_cancelled() {
            let payload = self.early_call_payload(
                &request,
                stream,
                LlmCallStatus::Cancelled,
                Some(LlmError::Cancelled.kind()),
                0,
                0,
            );
            scope.end_llm_span(span, &payload);
            scope.release_llm(reservation);
            return Err(LlmError::Cancelled);
        }
        if call_started >= scope.deadline() {
            let payload = self.early_call_payload(
                &request,
                stream,
                LlmCallStatus::TurnDeadlineExceeded,
                Some(LlmError::TurnDeadlineExceeded.kind()),
                0,
                0,
            );
            scope.end_llm_span(span, &payload);
            scope.release_llm(reservation);
            return Err(LlmError::TurnDeadlineExceeded);
        }

        let estimated_input = TokenAccountant::estimate_input_tokens(&request.messages);
        let max_output = u64::from(request.max_tokens);
        if let Err(error) = self
            .limiter
            .acquire_quota(estimated_input, max_output, scope.deadline(), scope.cancellation())
            .await
        {
            let payload =
                self.early_call_payload(&request, stream, llm_status_from_error(&error), Some(error.kind()), 0, 0);
            scope.end_llm_span(span, &payload);
            scope.release_llm(reservation);
            return Err(error);
        }

        let permit = match self.limiter.acquire_permit(scope.deadline(), scope.cancellation()).await {
            Ok(permit) => permit,
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_id = %scope.turn_id(),
                    stage = %scope.stage().as_str(),
                    purpose = request.purpose.as_str(),
                    queue_wait_ms = call_started.elapsed().as_millis(),
                    error_kind = error.kind(),
                    error = %error,
                    "llm call left the queue without reaching the provider"
                );
                let payload = self.early_call_payload(
                    &request,
                    stream,
                    llm_status_from_error(&error),
                    Some(error.kind()),
                    0,
                    call_started.elapsed().as_millis() as u64,
                );
                scope.end_llm_span(span, &payload);
                scope.release_llm(reservation);
                return Err(error);
            }
        };
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
            purpose = request.purpose.as_str(),
            provider = %self.provider.provider_name(),
            model = %self.config.model,
        );
        let provider_outcome: Result<LlmCompletion, LlmError> = match stream {
            false => {
                let call = self.provider.complete(&request);
                async {
                    tokio::select! {
                        result = call => result.map_err(LlmError::from),
                        _ = scope.cancellation().token().cancelled() => Err(LlmError::Cancelled),
                        _ = tokio::time::sleep_until(provider_deadline.into()) => {
                            if hits_turn_deadline {
                                Err(LlmError::TurnDeadlineExceeded)
                            } else {
                                Err(LlmError::ProviderTimeout)
                            }
                        }
                    }
                }
                .instrument(tracing_span)
                .await
            }
            true => {
                let call = self.provider.complete_stream(&request, sink.expect("stream sink checked"));
                async {
                    tokio::select! {
                        result = call => result.map_err(LlmError::from),
                        _ = scope.cancellation().token().cancelled() => Err(LlmError::Cancelled),
                        _ = tokio::time::sleep_until(provider_deadline.into()) => {
                            if hits_turn_deadline {
                                Err(LlmError::TurnDeadlineExceeded)
                            } else {
                                Err(LlmError::ProviderTimeout)
                            }
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
                if completion.text.trim().is_empty() {
                    let error = LlmError::Protocol {
                        kind: crate::llm::error::LlmProtocolErrorKind::EmptyChoices,
                    };
                    (None, Some(error))
                } else {
                    (Some(completion), None)
                }
            }
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_id = %scope.turn_id(),
                    stage = %scope.stage().as_str(),
                    purpose = request.purpose.as_str(),
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
        let charge_value = completion
            .as_ref()
            .and_then(|c| c.charge.as_ref())
            .and_then(|c| serde_json::to_value(c).ok());
        let finish_reason_owned = completion.as_ref().and_then(|c| c.finish_reason.clone());
        let finish_reason_string = finish_reason_owned.as_ref().map(|reason| reason.as_str().to_owned());
        let call_usage = self.usage_to_call_usage(request.purpose, &usage, &charge_value, finish_reason_owned);
        let settle = scope.settle_llm(reservation, call_usage);
        let usage_accuracy = usage.accuracy.as_str().to_owned();
        let status = if let Some(error) = &provider_error {
            llm_status_from_error(error)
        } else if settle.is_err() {
            LlmCallStatus::TokenBudgetExceeded
        } else if completion.is_some() {
            LlmCallStatus::Succeeded
        } else {
            LlmCallStatus::ProviderRejected
        };
        let error_kind = provider_error.as_ref().map(|error| error.kind().to_owned());
        let content = match self.config.trace_content {
            TraceContentPolicy::MetadataOnly => None,
            TraceContentPolicy::RedactedContent => Some(LlmCallContent {
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
            }),
            TraceContentPolicy::FullContent => Some(LlmCallContent {
                messages: request
                    .messages
                    .iter()
                    .map(|message| MessageData {
                        role: role_label(message.role).to_owned(),
                        content: message.content.clone(),
                    })
                    .collect(),
                response: completion.as_ref().map(|c| c.text.clone()).unwrap_or_default(),
            }),
        };
        let payload = SpanPayload::LlmCall(Box::new(LlmCallData {
            provider: self.provider.provider_name().to_owned(),
            model: request.model.clone(),
            purpose: request.purpose.as_str().to_owned(),
            stream,
            attempt: 1,
            queue_wait_ms,
            provider_latency_ms,
            total_latency_ms,
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            usage_accuracy,
            finish_reason: finish_reason_string,
            charge: charge_value,
            status: status.as_str().to_owned(),
            error_kind,
            content,
        }));
        scope.end_llm_span(span, &payload);
        drop(permit);
        settle.map_err(budget_to_llm)?;
        match completion {
            Some(completion) => Ok(completion),
            None => Err(provider_error.unwrap_or(LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
            })),
        }
    }

    fn early_call_payload(
        &self,
        request: &CompletionRequest,
        stream: bool,
        status: LlmCallStatus,
        error_kind: Option<&'static str>,
        estimated_input: u64,
        queue_wait_ms: u64,
    ) -> SpanPayload {
        SpanPayload::LlmCall(Box::new(LlmCallData {
            provider: self.provider.provider_name().to_owned(),
            model: self.config.model.clone(),
            purpose: request.purpose.as_str().to_owned(),
            stream,
            attempt: 1,
            queue_wait_ms,
            provider_latency_ms: 0,
            total_latency_ms: 0,
            input_tokens: estimated_input,
            cached_input_tokens: None,
            output_tokens: 0,
            reasoning_tokens: None,
            usage_accuracy: UsageAccuracy::Estimated.as_str().to_owned(),
            finish_reason: None,
            charge: None,
            status: status.as_str().to_owned(),
            error_kind: error_kind.map(str::to_owned),
            content: None,
        }))
    }

    fn usage_to_call_usage(
        &self,
        purpose: LlmCallPurpose,
        usage: &crate::llm::accounting::LlmTokenUsage,
        charge_value: &Option<serde_json::Value>,
        finish_reason: Option<FinishReason>,
    ) -> LlmCallUsage {
        let charge = charge_value
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        LlmCallUsage {
            call_id: crate::core::turn_contract::LlmCallId::new(),
            purpose,
            provider: self.provider.provider_name().to_owned(),
            model: self.config.model.clone(),
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            accuracy: usage.accuracy,
            pricing_version: None,
            charge,
            finish_reason,
        }
    }
}

fn estimated_usage(text: &str, estimated_input: u64) -> crate::llm::accounting::LlmTokenUsage {
    let output = estimate_text_tokens(text);
    crate::llm::accounting::LlmTokenUsage {
        input_tokens: estimated_input,
        cached_input_tokens: None,
        output_tokens: output,
        reasoning_tokens: None,
        total_tokens: estimated_input.saturating_add(output),
        accuracy: UsageAccuracy::Estimated,
    }
}

fn budget_to_llm(error: TurnExecutionError) -> LlmError {
    match error.kind() {
        crate::core::turn_error::TurnFailureKind::Cancelled => LlmError::Cancelled,
        crate::core::turn_error::TurnFailureKind::DeadlineExceeded => LlmError::TurnDeadlineExceeded,
        crate::core::turn_error::TurnFailureKind::TokenBudgetExceeded => {
            LlmError::TokenBudgetExceeded(error.to_string())
        }
        _ => LlmError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
        },
    }
}

fn llm_status_from_error(error: &LlmError) -> LlmCallStatus {
    match error {
        LlmError::Cancelled => LlmCallStatus::Cancelled,
        LlmError::TurnDeadlineExceeded => LlmCallStatus::TurnDeadlineExceeded,
        LlmError::ProviderTimeout => LlmCallStatus::ProviderTimeout,
        LlmError::QueueTimeout => LlmCallStatus::QueueTimeout,
        LlmError::RateLimited { .. } => LlmCallStatus::RateLimited,
        LlmError::TokenBudgetExceeded(_) => LlmCallStatus::TokenBudgetExceeded,
        LlmError::ProviderRejected { .. } => LlmCallStatus::ProviderRejected,
        LlmError::Transport { .. } => LlmCallStatus::TransportFailed,
        LlmError::Protocol { .. } => LlmCallStatus::ProtocolFailed,
        LlmError::ResponseLimitExceeded { .. } => LlmCallStatus::ResponseLimitExceeded,
        LlmError::EmbeddingUnsupported => LlmCallStatus::ProtocolFailed,
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
