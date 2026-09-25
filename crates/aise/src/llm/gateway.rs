use crate::config::{LlmConfig, ThinkingMode};
use crate::domain::text::estimate_text_tokens;
use crate::llm::accounting::{FinishReason, LlmCompletion, TokenAccountant};
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{
    ChatMessage, CompletionOutputSpec, CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role,
};
use crate::llm::observability;
use crate::llm::output_contract::{
    CompletionOutputRequest, LlmOutputContract, ResolvedStructuredOutputRequest, StructuredLlmCompletion,
    canonical_schema_hash, resolve_structured_output_mode,
};
use crate::llm::provider::{DeltaSink, LlmProvider};
use crate::observability::{ContentCapturePolicy, ObservabilityContentConfig, Observation};
use crate::prompt::{PromptComposition, PromptCompositionInput, TrustedPromptSource};
use crate::turn::turn_context::TurnLlmCallScope;
use crate::turn::turn_contract::{LlmBudgetReservation, LlmCallPurpose, LlmCallUsage, UsageAccuracy};
use crate::turn::turn_error::TurnExecutionError;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::Instrument;

pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    prompt_source: Arc<dyn TrustedPromptSource>,
    limiter: LlmLimiter,
    config: LlmConfig,
    accountant: TokenAccountant,
    observation_policy: ContentCapturePolicy,
    observation_limits: ObservabilityContentConfig,
}

#[derive(Debug, Clone, Copy)]
pub struct LlmObservation<'a> {
    pub attempt: u32,
    pub correction_round: Option<u32>,
    pub character_id: Option<&'a str>,
}

enum StructuredCheckOutcome {
    Decoded,
    DecodeFailed,
    ValidationFailed,
}

type StructuredCheck = Box<dyn FnOnce(&str) -> StructuredCheckOutcome + Send>;

struct CallOptions<'a> {
    structured: Option<StructuredCheck>,
    parent: &'a Observation,
}

impl LlmGateway {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        prompt_source: Arc<dyn TrustedPromptSource>,
        config: LlmConfig,
    ) -> Result<Self, TurnExecutionError> {
        config.validate().map_err(|error| {
            crate::turn::turn_error::TurnExecutionError::new(
                crate::turn::turn_error::TurnFailureKind::InvalidRequest,
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
            observation_policy: ContentCapturePolicy::MetadataOnly,
            observation_limits: ObservabilityContentConfig {
                policy: ContentCapturePolicy::MetadataOnly,
                max_field_bytes: 16_384,
                max_observation_bytes: 32_768,
                detector_overlap_bytes: 512,
            },
        })
    }

    pub fn with_observation_capture(
        mut self,
        policy: ContentCapturePolicy,
        mut limits: ObservabilityContentConfig,
    ) -> Self {
        self.observation_policy = policy.clone();
        limits.policy = policy;
        self.observation_limits = limits;
        self
    }

    pub async fn complete_text_composed(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        input: PromptCompositionInput,
        max_output_tokens: u32,
        purpose: LlmCallPurpose,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        let composition = self.render_composition(&input)?;
        let spec = CompletionSpec {
            messages: composition_messages(&composition),
            max_output_tokens,
            purpose,
            output: CompletionOutputSpec::Text,
        };
        let estimated_input = TokenAccountant::estimate_input_tokens(&spec.messages);
        let reservation = scope
            .reserve_llm(estimated_input, u64::from(spec.max_output_tokens))
            .map_err(|error| LlmError::TokenBudgetExceeded(error.to_string()))?;
        self.complete(scope, spec, reservation, parent).await
    }

    pub async fn complete_structured_composed<T>(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        input: PromptCompositionInput,
        max_output_tokens: u32,
        purpose: LlmCallPurpose,
        contract: LlmOutputContract<T>,
        parent: &Observation,
    ) -> Result<StructuredLlmCompletion<T>, LlmError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let configured_modes = self
            .config
            .structured_output
            .configured_modes(self.provider.provider_name(), &self.config.model);
        let mode = resolve_structured_output_mode(configured_modes, &self.provider.transport_capabilities()).map_err(
            |_| LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::StructuredOutputUnsupported,
            },
        )?;

        let composition = self.render_composition(&input)?;
        let mut messages = composition_messages(&composition);
        if mode.injects_prompt_contract() {
            let content = contract.compact_prompt_shape.as_ref().to_owned();
            messages.push(ChatMessage {
                role: Role::System,
                content,
            });
        }

        let schema_hash = canonical_schema_hash(&contract.schema);
        let resolved = ResolvedStructuredOutputRequest {
            contract_name: contract.name,
            schema: contract.schema.clone(),
            schema_hash,
            mode,
        };
        let validate = contract.validate.clone();
        let check: StructuredCheck = Box::new(move |text: &str| match serde_json::from_str::<T>(text) {
            Err(_) => StructuredCheckOutcome::DecodeFailed,
            Ok(value) => match validate(&value) {
                Ok(()) => StructuredCheckOutcome::Decoded,
                Err(_) => StructuredCheckOutcome::ValidationFailed,
            },
        });

        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: max_output_tokens,
            temperature: self.config.temperature,
            purpose,
            output: CompletionOutputRequest::Structured(resolved),
        };
        let estimated_input = TokenAccountant::estimate_input_tokens(&request.messages);
        let reservation = scope
            .reserve_llm(estimated_input, u64::from(max_output_tokens))
            .map_err(|error| LlmError::TokenBudgetExceeded(error.to_string()))?;

        let completion = self
            .run_call(
                &mut scope,
                request,
                false,
                None,
                reservation,
                CallOptions {
                    structured: Some(check),
                    parent,
                },
            )
            .await?;
        let value = serde_json::from_str::<T>(&completion.text).map_err(|_| LlmError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput,
        })?;
        (contract.validate)(&value).map_err(|_| LlmError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput,
        })?;
        Ok(StructuredLlmCompletion { value, completion })
    }

    fn render_composition(&self, input: &PromptCompositionInput) -> Result<PromptComposition, LlmError> {
        let render_started = Instant::now();
        let composition = self.prompt_source.compose(input).map_err(|_| LlmError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
        })?;
        let render_ms = render_started.elapsed().as_millis() as u64;
        tracing::info!(
            prompt_profile = %composition.profile,
            prompt_pack = %composition.metadata.csi.pack,
            csi_bytes = composition.csi.as_str().len(),
            csi_tokens = estimate_text_tokens(composition.csi.as_str()),
            rc_bytes = composition.rc.as_str().len(),
            rc_tokens = estimate_text_tokens(composition.rc.as_str()),
            fti_bytes = composition.fti.as_str().len(),
            fti_tokens = estimate_text_tokens(composition.fti.as_str()),
            render_ms,
            "prompt composition rendered"
        );
        Ok(composition)
    }

    pub async fn complete(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        let output = match spec.output {
            CompletionOutputSpec::Text => CompletionOutputRequest::Text,
            CompletionOutputSpec::Structured => {
                scope.release_llm(reservation);
                return Err(LlmError::Protocol {
                    kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
                });
            }
        };
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
            output,
        };
        self.execute_call(&mut scope, request, false, None, reservation, parent).await
    }

    pub async fn complete_stream(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        sink: DeltaSink,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        let output = match spec.output {
            CompletionOutputSpec::Text => CompletionOutputRequest::Text,
            CompletionOutputSpec::Structured => {
                scope.release_llm(reservation);
                return Err(LlmError::Protocol {
                    kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
                });
            }
        };
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: spec.messages,
            max_tokens: spec.max_output_tokens,
            temperature: self.config.temperature,
            purpose: spec.purpose,
            output,
        };
        self.execute_call_owned(scope, request, true, Some(sink), reservation, parent)
            .await
    }

    pub async fn embed(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        inputs: Vec<String>,
        reservation: LlmBudgetReservation,
        parent: &Observation,
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
        let call_id = reservation.call_id().clone();
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
            turn_number = %scope.turn_number(),
            stage = %scope.stage().as_str(),
            provider = %self.provider.provider_name(),
            model = %self.config.model,
        );
        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            inputs,
        };
        let mut generation = observability::begin_embedding(
            &self.observation_limits,
            self.provider.provider_name(),
            &scope,
            &request,
            parent,
        );
        let call = self.provider.embed(&request);
        let provider_outcome = async {
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
        .await;
        let total_latency_ms = call_started.elapsed().as_millis() as u64;
        let provider_latency_ms = total_latency_ms.saturating_sub(queue_wait_ms);

        let (output, provider_error) = match provider_outcome {
            Ok(output) => (Some(output), None),
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_number = %scope.turn_number(),
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
        let call_usage = self.usage_to_call_usage(call_id, LlmCallPurpose::Embedding, &usage, &charge_value, None);
        let observation_call_id = call_usage.call_id.as_str().to_owned();
        let settle = scope.settle_llm(reservation, call_usage);
        observability::record_call_metadata(
            &mut generation,
            observability::CallMetadata {
                call_id: observation_call_id.as_str(),
                queue_wait_ms,
                provider_latency_ms,
                total_latency_ms,
                usage_accuracy: usage.accuracy.as_str(),
                reasoning_content_available: None,
                finish_reason: None,
            },
        );
        observability::finish_call(generation, provider_error.as_ref(), Some(&usage));
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
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        if stream && sink.is_none() {
            return Err(LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::InvalidSseLine,
            });
        }
        self.run_call(
            scope,
            request,
            stream,
            sink,
            reservation,
            CallOptions {
                structured: None,
                parent,
            },
        )
        .await
    }

    async fn execute_call_owned(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        self.run_call(
            &mut scope,
            request,
            stream,
            sink,
            reservation,
            CallOptions {
                structured: None,
                parent,
            },
        )
        .await
    }

    async fn run_call(
        &self,
        scope: &mut TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
        options: CallOptions<'_>,
    ) -> Result<LlmCompletion, LlmError> {
        let structured_check = options.structured;
        let call_id = reservation.call_id().clone();
        let mut generation = observability::begin_generation(
            self.observation_policy.clone(),
            &self.observation_limits,
            self.provider.provider_name(),
            thinking_mode(self.config.thinking),
            scope,
            &request,
            options.parent,
        );
        let call_started = Instant::now();

        if scope.cancellation().is_cancelled() {
            observability::finish_call(generation, Some(&LlmError::Cancelled), None);
            scope.release_llm(reservation);
            return Err(LlmError::Cancelled);
        }
        if call_started >= scope.deadline() {
            observability::finish_call(generation, Some(&LlmError::TurnDeadlineExceeded), None);
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
            observability::finish_call(generation, Some(&error), None);
            scope.release_llm(reservation);
            return Err(error);
        }

        let permit = match self.limiter.acquire_permit(scope.deadline(), scope.cancellation()).await {
            Ok(permit) => permit,
            Err(error) => {
                tracing::warn!(
                    story_id = %scope.story_id(),
                    turn_number = %scope.turn_number(),
                    stage = %scope.stage().as_str(),
                    purpose = request.purpose.as_str(),
                    queue_wait_ms = call_started.elapsed().as_millis(),
                    error_kind = error.kind(),
                    error = %error,
                    "llm call left the queue without reaching the provider"
                );
                observability::finish_call(generation, Some(&error), None);
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
            turn_number = %scope.turn_number(),
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

        let (mut completion, mut provider_error) = match provider_outcome {
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
                    turn_number = %scope.turn_number(),
                    stage = %scope.stage().as_str(),
                    purpose = request.purpose.as_str(),
                    error_kind = error.kind(),
                    error = %error,
                    "llm call failed"
                );
                (None, Some(error))
            }
        };

        if let (Some(check), Some(seen)) = (structured_check, completion.as_ref()) {
            if !matches!(check(&seen.text), StructuredCheckOutcome::Decoded) {
                completion = None;
                provider_error = Some(LlmError::Protocol {
                    kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput,
                });
            }
        }

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
        let call_usage = self.usage_to_call_usage(call_id, request.purpose, &usage, &charge_value, finish_reason_owned);
        let observation_call_id = call_usage.call_id.as_str().to_owned();
        let settle = scope.settle_llm(reservation, call_usage);
        observability::record_call_metadata(
            &mut generation,
            observability::CallMetadata {
                call_id: observation_call_id.as_str(),
                queue_wait_ms,
                provider_latency_ms,
                total_latency_ms,
                usage_accuracy: usage.accuracy.as_str(),
                reasoning_content_available: Some(usage.reasoning_tokens.unwrap_or_default() > 0),
                finish_reason: finish_reason_string.as_deref(),
            },
        );
        observability::finish_generation(
            &self.observation_limits,
            generation,
            completion.as_ref(),
            provider_error.as_ref(),
            &usage,
        );
        drop(permit);
        settle.map_err(budget_to_llm)?;
        match completion {
            Some(completion) => Ok(completion),
            None => Err(provider_error.unwrap_or(LlmError::Protocol {
                kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
            })),
        }
    }

    fn usage_to_call_usage(
        &self,
        call_id: crate::turn::turn_contract::LlmCallId,
        purpose: LlmCallPurpose,
        usage: &crate::llm::accounting::LlmTokenUsage,
        charge_value: &Option<serde_json::Value>,
        finish_reason: Option<FinishReason>,
    ) -> LlmCallUsage {
        let charge = charge_value
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        LlmCallUsage {
            call_id,
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

const fn thinking_mode(mode: Option<ThinkingMode>) -> &'static str {
    match mode {
        Some(ThinkingMode::Enabled) => "enabled",
        Some(ThinkingMode::Disabled) => "disabled",
        None => "provider_default",
    }
}

fn composition_messages(composition: &PromptComposition) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: Role::System,
            content: composition.csi.as_str().to_owned(),
        },
        ChatMessage {
            role: Role::User,
            content: composition.rc.as_str().to_owned(),
        },
        ChatMessage {
            role: Role::System,
            content: composition.fti.as_str().to_owned(),
        },
    ]
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
        crate::turn::turn_error::TurnFailureKind::Cancelled => LlmError::Cancelled,
        crate::turn::turn_error::TurnFailureKind::DeadlineExceeded => LlmError::TurnDeadlineExceeded,
        crate::turn::turn_error::TurnFailureKind::TokenBudgetExceeded => {
            LlmError::TokenBudgetExceeded(error.to_string())
        }
        _ => LlmError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::Unsupported,
        },
    }
}
