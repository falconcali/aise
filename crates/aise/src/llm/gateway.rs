use crate::config::{LlmConfig, ThinkingMode, TraceContentPolicy};
use crate::domain::text::estimate_text_tokens;
use crate::llm::accounting::{FinishReason, LlmCompletion, TokenAccountant};
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{
    ChatMessage, CompletionOutputSpec, CompletionRequest, CompletionSpec, EmbeddingOutput, EmbeddingRequest, Role,
};
use crate::llm::output_contract::{
    CompletionOutputRequest, LlmOutputContract, ResolvedStructuredOutputRequest, StructuredLlmCompletion,
    canonical_schema_hash, resolve_structured_output_mode,
};
use crate::llm::provider::{DeltaSink, LlmProvider};
use crate::prompt::{PromptComposition, PromptCompositionInput, TrustedPromptSource};
use crate::turn::observability::{
    BoundedContentEncoder, ContentCaptureLimits, ContentCapturePolicy, GenerationUsage, METADATA_ATTEMPT,
    METADATA_CALL_ID, METADATA_CHARACTER_ID, METADATA_CONTENT_ENCODE_FAILED, METADATA_CORRECTION_ROUND,
    METADATA_FINISH_REASON, METADATA_PROVIDER, METADATA_PROVIDER_LATENCY_MS, METADATA_QUEUE_WAIT_MS,
    METADATA_REASONING_CONTENT_AVAILABLE, METADATA_TOTAL_LATENCY_MS, METADATA_USAGE_ACCURACY, OBSERVATION_MODEL_NAME,
    OBSERVATION_MODEL_PARAMETERS, ObservationAttribute, ObservationError, ObservationFields, ObservationFinish,
    ObservationSpan, ObservationStatus,
};
use crate::turn::turn_context::TurnLlmCallScope;
use crate::turn::turn_contract::{LlmBudgetReservation, LlmCallPurpose, LlmCallStatus, LlmCallUsage, UsageAccuracy};
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_trace::{
    LlmCallContent, LlmCallData, MAX_LLM_CONTENT_CHARS, MAX_LLM_RESPONSE_CHARS, MessageData, SpanPayload,
    StructuredCallData, truncate,
};
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
    observation_limits: ContentCaptureLimits,
}

#[derive(Debug, Clone, Copy)]
pub struct LlmObservation<'a> {
    pub step: crate::turn::observability::ObservationStep,
    pub attempt: u32,
    pub correction_round: Option<u32>,
    pub character_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct StructuredCallMeta {
    contract_name: String,
    schema_hash: String,
    mode: String,
    schema_bytes: usize,
    prompt_contract_bytes: usize,
}

enum StructuredCheckOutcome {
    Decoded,
    DecodeFailed,
    ValidationFailed,
}

type StructuredCheck = Box<dyn FnOnce(&str) -> StructuredCheckOutcome + Send>;

struct EarlyCallPayloadArgs<'a> {
    request: &'a CompletionRequest,
    stream: bool,
    status: LlmCallStatus,
    error_kind: Option<&'static str>,
    estimated_input: u64,
    queue_wait_ms: u64,
    structured_meta: Option<&'a StructuredCallMeta>,
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
            observation_limits: ContentCaptureLimits {
                max_field_bytes: 16_384,
                max_observation_bytes: 32_768,
                detector_overlap_bytes: 512,
            },
        })
    }

    pub fn with_observation_capture(mut self, policy: ContentCapturePolicy, limits: ContentCaptureLimits) -> Self {
        self.observation_policy = policy;
        self.observation_limits = limits;
        self
    }

    pub async fn complete_text_composed(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        input: PromptCompositionInput,
        max_output_tokens: u32,
        purpose: LlmCallPurpose,
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
        self.complete(scope, spec, reservation).await
    }

    pub async fn complete_structured_composed<T>(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        input: PromptCompositionInput,
        max_output_tokens: u32,
        purpose: LlmCallPurpose,
        contract: LlmOutputContract<T>,
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
        let prompt_contract_bytes = if mode.injects_prompt_contract() {
            let content = contract.compact_prompt_shape.as_ref().to_owned();
            let bytes = content.len();
            messages.push(ChatMessage {
                role: Role::System,
                content,
            });
            bytes
        } else {
            0
        };

        let schema_hash = canonical_schema_hash(&contract.schema);
        let schema_bytes = serde_json::to_string(contract.schema.as_ref()).map(|s| s.len()).unwrap_or(0);
        let meta = StructuredCallMeta {
            contract_name: contract.name.to_owned(),
            schema_hash: schema_hash.to_string(),
            mode: mode.as_str().to_owned(),
            schema_bytes,
            prompt_contract_bytes,
        };
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
            .run_call(&mut scope, request, false, None, reservation, Some((meta, check)))
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
        self.execute_call(&mut scope, request, false, None, reservation).await
    }

    pub async fn complete_stream(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        sink: DeltaSink,
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
        let call_id = reservation.call_id().clone();
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
            self.observation_policy,
            &self.observation_limits,
            self.provider.provider_name(),
            &scope,
            &request,
        );
        let provider_outcome = generation
            .in_scope(async {
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
        let mut generation = generation;
        generation.record_attribute(ObservationAttribute::string(METADATA_CALL_ID, observation_call_id));
        generation.record_attribute(ObservationAttribute::u64(METADATA_QUEUE_WAIT_MS, queue_wait_ms));
        generation.record_attribute(ObservationAttribute::u64(METADATA_PROVIDER_LATENCY_MS, provider_latency_ms));
        generation.record_attribute(ObservationAttribute::u64(METADATA_TOTAL_LATENCY_MS, total_latency_ms));
        generation.record_attribute(ObservationAttribute::string(METADATA_USAGE_ACCURACY, usage.accuracy.as_str()));
        generation.finish(ObservationFinish {
            status: provider_error
                .as_ref()
                .map(llm_observation_status)
                .unwrap_or(ObservationStatus::Ok),
            error: provider_error.as_ref().map(llm_observation_error),
            usage: Some(generation_usage(&usage)),
            ..ObservationFinish::default()
        });
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
            structured_output: None,
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
        self.run_call(scope, request, stream, sink, reservation, None).await
    }

    async fn execute_call_owned(
        &self,
        mut scope: TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError> {
        self.run_call(&mut scope, request, stream, sink, reservation, None).await
    }

    async fn run_call(
        &self,
        scope: &mut TurnLlmCallScope<'_>,
        request: CompletionRequest,
        stream: bool,
        sink: Option<DeltaSink>,
        reservation: LlmBudgetReservation,
        structured: Option<(StructuredCallMeta, StructuredCheck)>,
    ) -> Result<LlmCompletion, LlmError> {
        let (structured_meta, structured_check) = match structured {
            Some((meta, check)) => (Some(meta), Some(check)),
            None => (None, None),
        };
        let call_id = reservation.call_id().clone();
        let mut generation = begin_generation(
            self.observation_policy,
            &self.observation_limits,
            self.provider.provider_name(),
            thinking_mode(self.config.thinking),
            scope,
            &request,
        );
        let span = scope.begin_llm_span();
        let call_started = Instant::now();

        if scope.cancellation().is_cancelled() {
            let payload = self.early_call_payload(EarlyCallPayloadArgs {
                request: &request,
                stream,
                status: LlmCallStatus::Cancelled,
                error_kind: Some(LlmError::Cancelled.kind()),
                estimated_input: 0,
                queue_wait_ms: 0,
                structured_meta: structured_meta.as_ref(),
            });
            generation.finish(ObservationFinish {
                status: ObservationStatus::Cancelled,
                error: Some(llm_observation_error(&LlmError::Cancelled)),
                ..ObservationFinish::default()
            });
            scope.end_llm_span(span, &payload);
            scope.release_llm(reservation);
            return Err(LlmError::Cancelled);
        }
        if call_started >= scope.deadline() {
            let payload = self.early_call_payload(EarlyCallPayloadArgs {
                request: &request,
                stream,
                status: LlmCallStatus::TurnDeadlineExceeded,
                error_kind: Some(LlmError::TurnDeadlineExceeded.kind()),
                estimated_input: 0,
                queue_wait_ms: 0,
                structured_meta: structured_meta.as_ref(),
            });
            generation.finish(ObservationFinish {
                status: ObservationStatus::DeadlineExceeded,
                error: Some(llm_observation_error(&LlmError::TurnDeadlineExceeded)),
                ..ObservationFinish::default()
            });
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
            let payload = self.early_call_payload(EarlyCallPayloadArgs {
                request: &request,
                stream,
                status: llm_status_from_error(&error),
                error_kind: Some(error.kind()),
                estimated_input: 0,
                queue_wait_ms: 0,
                structured_meta: structured_meta.as_ref(),
            });
            generation.finish(ObservationFinish {
                status: llm_observation_status(&error),
                error: Some(llm_observation_error(&error)),
                ..ObservationFinish::default()
            });
            scope.end_llm_span(span, &payload);
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
                let payload = self.early_call_payload(EarlyCallPayloadArgs {
                    request: &request,
                    stream,
                    status: llm_status_from_error(&error),
                    error_kind: Some(error.kind()),
                    estimated_input: 0,
                    queue_wait_ms: call_started.elapsed().as_millis() as u64,
                    structured_meta: structured_meta.as_ref(),
                });
                generation.finish(ObservationFinish {
                    status: llm_observation_status(&error),
                    error: Some(llm_observation_error(&error)),
                    ..ObservationFinish::default()
                });
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
            turn_number = %scope.turn_number(),
            stage = %scope.stage().as_str(),
            purpose = request.purpose.as_str(),
            provider = %self.provider.provider_name(),
            model = %self.config.model,
        );
        let provider_outcome: Result<LlmCompletion, LlmError> = generation
            .in_scope(async {
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

        let mut decode_status: Option<&'static str> = None;
        let mut validation_status: Option<&'static str> = None;
        if let (Some(check), Some(seen)) = (structured_check, completion.as_ref()) {
            match check(&seen.text) {
                StructuredCheckOutcome::Decoded => {
                    decode_status = Some("ok");
                    validation_status = Some("ok");
                }
                StructuredCheckOutcome::DecodeFailed => {
                    decode_status = Some("invalid_json");
                    validation_status = Some("skipped");
                    completion = None;
                    provider_error = Some(LlmError::Protocol {
                        kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput,
                    });
                }
                StructuredCheckOutcome::ValidationFailed => {
                    decode_status = Some("ok");
                    validation_status = Some("violated");
                    completion = None;
                    provider_error = Some(LlmError::Protocol {
                        kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput,
                    });
                }
            }
        } else if structured_meta.is_some() {
            decode_status = Some("not_attempted");
            validation_status = Some("not_attempted");
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
        generation.record_attribute(ObservationAttribute::string(METADATA_CALL_ID, observation_call_id));
        generation.record_attribute(ObservationAttribute::u64(METADATA_QUEUE_WAIT_MS, queue_wait_ms));
        generation.record_attribute(ObservationAttribute::u64(METADATA_PROVIDER_LATENCY_MS, provider_latency_ms));
        generation.record_attribute(ObservationAttribute::u64(METADATA_TOTAL_LATENCY_MS, total_latency_ms));
        generation.record_attribute(ObservationAttribute::string(METADATA_USAGE_ACCURACY, usage.accuracy.as_str()));
        generation.record_attribute(ObservationAttribute::bool(
            METADATA_REASONING_CONTENT_AVAILABLE,
            usage.reasoning_tokens.unwrap_or_default() > 0,
        ));
        if let Some(reason) = &finish_reason_string {
            generation.record_attribute(ObservationAttribute::string(METADATA_FINISH_REASON, reason));
        }
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
        let structured_output = structured_meta.map(|meta| StructuredCallData {
            output_contract: meta.contract_name,
            schema_hash: meta.schema_hash,
            structured_output_mode: meta.mode,
            schema_bytes: meta.schema_bytes,
            prompt_contract_bytes: meta.prompt_contract_bytes,
            decode_status: decode_status.unwrap_or("not_attempted").to_owned(),
            validation_status: validation_status.unwrap_or("not_attempted").to_owned(),
        });
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
            structured_output,
        }));
        finish_generation(
            self.observation_policy,
            &self.observation_limits,
            generation,
            completion.as_ref(),
            provider_error.as_ref(),
            &usage,
        );
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

    fn early_call_payload(&self, args: EarlyCallPayloadArgs<'_>) -> SpanPayload {
        let EarlyCallPayloadArgs {
            request,
            stream,
            status,
            error_kind,
            estimated_input,
            queue_wait_ms,
            structured_meta,
        } = args;
        let structured_output = structured_meta.map(|meta| StructuredCallData {
            output_contract: meta.contract_name.clone(),
            schema_hash: meta.schema_hash.clone(),
            structured_output_mode: meta.mode.clone(),
            schema_bytes: meta.schema_bytes,
            prompt_contract_bytes: meta.prompt_contract_bytes,
            decode_status: "not_attempted".to_owned(),
            validation_status: "not_attempted".to_owned(),
        });
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
            structured_output,
        }))
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
    policy: ContentCapturePolicy,
    limits: &ContentCaptureLimits,
    provider: &str,
    thinking: &'static str,
    scope: &TurnLlmCallScope<'_>,
    request: &CompletionRequest,
) -> crate::turn::observability::ObservationSpan {
    let encoder = BoundedContentEncoder::new(policy, limits.clone());
    let parameters = serde_json::json!({
        "temperature": request.temperature,
        "max_tokens": request.max_tokens,
        "thinking": thinking,
    })
    .to_string();
    let mut metadata = vec![
        ObservationAttribute::string(METADATA_PROVIDER, provider),
        ObservationAttribute::u64(METADATA_ATTEMPT, scope.attempt() as u64),
        ObservationAttribute::u64(METADATA_QUEUE_WAIT_MS, 0),
        ObservationAttribute::u64(METADATA_PROVIDER_LATENCY_MS, 0),
        ObservationAttribute::u64(METADATA_TOTAL_LATENCY_MS, 0),
        ObservationAttribute::string(OBSERVATION_MODEL_NAME, request.model.clone()),
        ObservationAttribute::string(OBSERVATION_MODEL_PARAMETERS, parameters),
    ];
    if let Some(round) = scope.correction_round() {
        metadata.push(ObservationAttribute::u64(METADATA_CORRECTION_ROUND, round as u64));
    }
    if let Some(character_id) = scope.character_id() {
        metadata.push(ObservationAttribute::string(METADATA_CHARACTER_ID, character_id));
    }
    let mut observation = ObservationSpan::begin(scope.observation_step(), ObservationFields { metadata, input: None });
    if observation.is_recording() {
        let (input, encoding_failed) = encoder.encode_with_status(&request.messages, limits.max_observation_bytes);
        if encoding_failed {
            observation.record_attribute(ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
        }
        observation.record_fields(ObservationFields {
            metadata: Vec::new(),
            input,
        });
    }
    observation
}

fn begin_embedding_generation(
    policy: ContentCapturePolicy,
    limits: &ContentCaptureLimits,
    provider: &str,
    scope: &TurnLlmCallScope<'_>,
    request: &EmbeddingRequest,
) -> crate::turn::observability::ObservationSpan {
    let encoder = BoundedContentEncoder::new(policy, limits.clone());
    let mut metadata = vec![
        ObservationAttribute::string(METADATA_PROVIDER, provider),
        ObservationAttribute::u64(METADATA_ATTEMPT, scope.attempt() as u64),
        ObservationAttribute::string(OBSERVATION_MODEL_NAME, request.model.clone()),
        ObservationAttribute::string(
            OBSERVATION_MODEL_PARAMETERS,
            serde_json::json!({"thinking": "provider_default"}).to_string(),
        ),
    ];
    if let Some(round) = scope.correction_round() {
        metadata.push(ObservationAttribute::u64(METADATA_CORRECTION_ROUND, round as u64));
    }
    let mut observation = ObservationSpan::begin(scope.observation_step(), ObservationFields { metadata, input: None });
    if observation.is_recording() {
        let (input, encoding_failed) = encoder.encode_with_status(&request.inputs, limits.max_observation_bytes);
        if encoding_failed {
            observation.record_attribute(ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
        }
        observation.record_fields(ObservationFields {
            metadata: Vec::new(),
            input,
        });
    }
    observation
}

fn finish_generation(
    policy: ContentCapturePolicy,
    limits: &ContentCaptureLimits,
    generation: crate::turn::observability::ObservationSpan,
    completion: Option<&LlmCompletion>,
    error: Option<&LlmError>,
    usage: &crate::llm::accounting::LlmTokenUsage,
) {
    let encoder = BoundedContentEncoder::new(policy, limits.clone());
    let (output, encoding_failed) = if generation.is_recording() {
        completion
            .map(|completion| encoder.encode_with_status(&completion.text, limits.max_observation_bytes))
            .unwrap_or((None, false))
    } else {
        (None, false)
    };
    let status = error.map(llm_observation_status).unwrap_or(ObservationStatus::Ok);
    let error = error.map(llm_observation_error);
    let metadata = if encoding_failed {
        vec![ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true)]
    } else {
        Vec::new()
    };
    generation.finish(ObservationFinish {
        status,
        metadata,
        output,
        error,
        usage: Some(generation_usage(usage)),
        ..ObservationFinish::default()
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
