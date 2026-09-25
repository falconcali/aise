use crate::config::{LlmConfig, ThinkingMode};
use crate::domain::text::estimate_text_tokens;
use crate::llm::accounting::{FinishReason, LlmCompletion, TokenAccountant};
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{
    ChatMessage, CompletionOutputSpec, CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role,
};
use crate::llm::observability::{
    METADATA_ATTEMPT, METADATA_CALL_ID, METADATA_CHARACTER_ID, METADATA_CONTENT_ENCODE_FAILED,
    METADATA_CORRECTION_ROUND, METADATA_FINISH_REASON, METADATA_PROVIDER, METADATA_PROVIDER_LATENCY_MS,
    METADATA_QUEUE_WAIT_MS, METADATA_REASONING_CONTENT_AVAILABLE, METADATA_TOTAL_LATENCY_MS, METADATA_USAGE_ACCURACY,
};
use crate::llm::output_contract::{
    CompletionOutputRequest, LlmOutputContract, ResolvedStructuredOutputRequest, StructuredLlmCompletion,
    canonical_schema_hash, resolve_structured_output_mode,
};
use crate::llm::provider::{DeltaSink, LlmProvider};
use crate::observability::{
    Attribute, ContentCapture, ContentCapturePolicy, GenerationUsage, OBSERVATION_MODEL_NAME,
    OBSERVATION_MODEL_PARAMETERS, ObservabilityContentConfig, Observation, ObservationError, ObservationOutcome,
    ObservationSpec, ObservationStatus,
};
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
            .run_call(&mut scope, request, false, None, reservation, Some(check), parent)
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
        let generation = begin_embedding_generation(
            self.observation_policy.clone(),
            &self.observation_limits,
            self.provider.provider_name(),
            &scope,
            &request,
            parent,
        );
        let provider_outcome = generation
            .trace(async {
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
            })
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
        let mut generation = generation;
        generation.record_attribute(Attribute::string(METADATA_CALL_ID, observation_call_id));
        generation.record_attribute(Attribute::u64(METADATA_QUEUE_WAIT_MS, queue_wait_ms));
        generation.record_attribute(Attribute::u64(METADATA_PROVIDER_LATENCY_MS, provider_latency_ms));
        generation.record_attribute(Attribute::u64(METADATA_TOTAL_LATENCY_MS, total_latency_ms));
        generation.record_attribute(Attribute::string(METADATA_USAGE_ACCURACY, usage.accuracy.as_str()));
        generation.finish(ObservationOutcome {
            status: provider_error
                .as_ref()
                .map(llm_observation_status)
                .unwrap_or(ObservationStatus::Ok),
            error: provider_error.as_ref().map(llm_observation_error),
            usage: Some(generation_usage(&usage)),
            ..ObservationOutcome::default()
        });
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
        self.run_call(scope, request, stream, sink, reservation, None, parent).await
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
        self.run_call(&mut scope, request, stream, sink, reservation, None, parent)
            .await
    }

    async fn run_call(
        &self,
        scope: &mut TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
        structured: Option<StructuredCheck>,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError> {
        let structured_check = structured;
        let call_id = reservation.call_id().clone();
        let mut generation = begin_generation(
            self.observation_policy.clone(),
            &self.observation_limits,
            self.provider.provider_name(),
            thinking_mode(self.config.thinking),
            scope,
            &request,
            parent,
        );
        let call_started = Instant::now();

        if scope.cancellation().is_cancelled() {
            generation.finish(ObservationOutcome {
                status: ObservationStatus::Cancelled,
                error: Some(llm_observation_error(&LlmError::Cancelled)),
                ..ObservationOutcome::default()
            });
            scope.release_llm(reservation);
            return Err(LlmError::Cancelled);
        }
        if call_started >= scope.deadline() {
            generation.finish(ObservationOutcome {
                status: ObservationStatus::DeadlineExceeded,
                error: Some(llm_observation_error(&LlmError::TurnDeadlineExceeded)),
                ..ObservationOutcome::default()
            });
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
            generation.finish(ObservationOutcome {
                status: llm_observation_status(&error),
                error: Some(llm_observation_error(&error)),
                ..ObservationOutcome::default()
            });
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
                generation.finish(ObservationOutcome {
                    status: llm_observation_status(&error),
                    error: Some(llm_observation_error(&error)),
                    ..ObservationOutcome::default()
                });
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
        let provider_outcome: Result<LlmCompletion, LlmError> = generation
            .trace(async {
                match stream {
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
                }
            })
            .await;
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
        generation.record_attribute(Attribute::string(METADATA_CALL_ID, observation_call_id));
        generation.record_attribute(Attribute::u64(METADATA_QUEUE_WAIT_MS, queue_wait_ms));
        generation.record_attribute(Attribute::u64(METADATA_PROVIDER_LATENCY_MS, provider_latency_ms));
        generation.record_attribute(Attribute::u64(METADATA_TOTAL_LATENCY_MS, total_latency_ms));
        generation.record_attribute(Attribute::string(METADATA_USAGE_ACCURACY, usage.accuracy.as_str()));
        generation.record_attribute(Attribute::bool(
            METADATA_REASONING_CONTENT_AVAILABLE,
            usage.reasoning_tokens.unwrap_or_default() > 0,
        ));
        if let Some(reason) = &finish_reason_string {
            generation.record_attribute(Attribute::string(METADATA_FINISH_REASON, reason));
        }
        finish_generation(
            self.observation_policy.clone(),
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

fn begin_generation(
    _policy: ContentCapturePolicy,
    limits: &ObservabilityContentConfig,
    provider: &str,
    thinking: &'static str,
    scope: &TurnLlmCallScope<'_>,
    request: &CompletionRequest,
    parent: &Observation,
) -> Observation {
    let encoder = ContentCapture::new(limits.clone());
    let parameters = serde_json::json!({
        "temperature": request.temperature,
        "max_tokens": request.max_tokens,
        "thinking": thinking,
    })
    .to_string();
    let mut metadata = vec![
        Attribute::string(METADATA_PROVIDER, provider),
        Attribute::u64(METADATA_ATTEMPT, scope.attempt() as u64),
        Attribute::u64(METADATA_QUEUE_WAIT_MS, 0),
        Attribute::u64(METADATA_PROVIDER_LATENCY_MS, 0),
        Attribute::u64(METADATA_TOTAL_LATENCY_MS, 0),
        Attribute::string(OBSERVATION_MODEL_NAME, request.model.clone()),
        Attribute::string(OBSERVATION_MODEL_PARAMETERS, parameters),
    ];
    if let Some(round) = scope.correction_round() {
        metadata.push(Attribute::u64(METADATA_CORRECTION_ROUND, round as u64));
    }
    if let Some(character_id) = scope.character_id() {
        metadata.push(Attribute::string(METADATA_CHARACTER_ID, character_id));
    }
    let captured = encoder.encode(&request.messages, limits.max_observation_bytes);
    let mut spec = ObservationSpec {
        name: "llm-generation",
        kind: crate::observability::ObservationKind::Generation,
        input: captured.content,
        metadata,
    };
    if captured.encode_failed {
        spec.metadata.push(Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
    }
    parent.begin(spec)
}

fn begin_embedding_generation(
    _policy: ContentCapturePolicy,
    limits: &ObservabilityContentConfig,
    provider: &str,
    scope: &TurnLlmCallScope<'_>,
    request: &EmbeddingRequest,
    parent: &Observation,
) -> Observation {
    let encoder = ContentCapture::new(limits.clone());
    let mut metadata = vec![
        Attribute::string(METADATA_PROVIDER, provider),
        Attribute::u64(METADATA_ATTEMPT, scope.attempt() as u64),
        Attribute::string(OBSERVATION_MODEL_NAME, request.model.clone()),
        Attribute::string(
            OBSERVATION_MODEL_PARAMETERS,
            serde_json::json!({"thinking": "provider_default"}).to_string(),
        ),
    ];
    if let Some(round) = scope.correction_round() {
        metadata.push(Attribute::u64(METADATA_CORRECTION_ROUND, round as u64));
    }
    let captured = encoder.encode(&request.inputs, limits.max_observation_bytes);
    let mut spec = ObservationSpec {
        name: "llm-embedding",
        kind: crate::observability::ObservationKind::Generation,
        input: captured.content,
        metadata,
    };
    if captured.encode_failed {
        spec.metadata.push(Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
    }
    parent.begin(spec)
}

fn finish_generation(
    _policy: ContentCapturePolicy,
    limits: &ObservabilityContentConfig,
    generation: Observation,
    completion: Option<&LlmCompletion>,
    error: Option<&LlmError>,
    usage: &crate::llm::accounting::LlmTokenUsage,
) {
    let encoder = ContentCapture::new(limits.clone());
    let (output, encoding_failed) = if generation.is_recording() {
        completion
            .map(|completion| {
                let captured = encoder.encode(&completion.text, limits.max_observation_bytes);
                (captured.content, captured.encode_failed)
            })
            .unwrap_or((None, false))
    } else {
        (None, false)
    };
    let status = error.map(llm_observation_status).unwrap_or(ObservationStatus::Ok);
    let error = error.map(llm_observation_error);
    let metadata = if encoding_failed {
        vec![Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true)]
    } else {
        Vec::new()
    };
    generation.finish(ObservationOutcome {
        status,
        metadata,
        output,
        error,
        usage: Some(generation_usage(usage)),
        ..ObservationOutcome::default()
    });
}

fn generation_usage(usage: &crate::llm::accounting::LlmTokenUsage) -> GenerationUsage {
    let cached = usage.cached_input_tokens.unwrap_or_default().min(usage.input_tokens);
    let reasoning = usage.reasoning_tokens.unwrap_or_default().min(usage.output_tokens);
    let input = usage.input_tokens.saturating_sub(cached);
    let output = usage.output_tokens.saturating_sub(reasoning);
    let total = input.saturating_add(cached).saturating_add(output).saturating_add(reasoning);
    GenerationUsage {
        input,
        input_cached_tokens: cached,
        output,
        output_reasoning_tokens: reasoning,
        total,
    }
}

fn llm_observation_status(error: &LlmError) -> ObservationStatus {
    match error {
        LlmError::Cancelled => ObservationStatus::Cancelled,
        LlmError::TurnDeadlineExceeded | LlmError::ProviderTimeout => ObservationStatus::DeadlineExceeded,
        _ => ObservationStatus::Error,
    }
}

fn llm_observation_error(error: &LlmError) -> ObservationError {
    ObservationError {
        code: error.kind().to_owned(),
        failure_kind: "llm".into(),
        stage: None,
        message: error.to_string(),
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
