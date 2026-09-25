pub const METADATA_ATTEMPT: &str = "aise.observation.metadata.attempt";
pub const METADATA_CALL_ID: &str = "aise.observation.metadata.call_id";
pub const METADATA_CHARACTER_ID: &str = "aise.observation.metadata.character_id";
pub const METADATA_CONTENT_ENCODE_FAILED: &str = "aise.observation.metadata.content_encode_failed";
pub const METADATA_CORRECTION_ROUND: &str = "aise.observation.metadata.correction_round";
pub const METADATA_FINISH_REASON: &str = "aise.observation.metadata.finish_reason";
pub const METADATA_PROVIDER: &str = "aise.observation.metadata.provider";
pub const METADATA_PROVIDER_LATENCY_MS: &str = "aise.observation.metadata.provider_latency_ms";
pub const METADATA_QUEUE_WAIT_MS: &str = "aise.observation.metadata.queue_wait_ms";
pub const METADATA_REASONING_CONTENT_AVAILABLE: &str = "aise.observation.metadata.reasoning_content_available";
pub const METADATA_TOTAL_LATENCY_MS: &str = "aise.observation.metadata.total_latency_ms";
pub const METADATA_USAGE_ACCURACY: &str = "aise.observation.metadata.usage_accuracy";

use crate::llm::accounting::{LlmCompletion, LlmTokenUsage};
use crate::llm::error::LlmError;
use crate::llm::message::{CompletionRequest, EmbeddingRequest};
use crate::observability::{
    Attribute, ContentCapture, ContentCapturePolicy, GenerationUsage, OBSERVATION_MODEL_NAME,
    OBSERVATION_MODEL_PARAMETERS, ObservabilityContentConfig, Observation, ObservationError, ObservationKind,
    ObservationOutcome, ObservationSpec, ObservationStatus,
};
use crate::turn::turn_context::TurnLlmCallScope;

pub(crate) fn begin_generation(
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
    if captured.encode_failed {
        metadata.push(Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
    }
    parent.begin(ObservationSpec {
        name: "llm-generation",
        kind: ObservationKind::Generation,
        input: captured.content,
        metadata,
    })
}

pub(crate) fn begin_embedding(
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
    if captured.encode_failed {
        metadata.push(Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
    }
    parent.begin(ObservationSpec {
        name: "llm-embedding",
        kind: ObservationKind::Generation,
        input: captured.content,
        metadata,
    })
}

pub(crate) fn finish_generation(
    limits: &ObservabilityContentConfig,
    generation: Observation,
    completion: Option<&LlmCompletion>,
    error: Option<&LlmError>,
    usage: &LlmTokenUsage,
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
    generation.finish(ObservationOutcome {
        status: error.map(observation_status).unwrap_or(ObservationStatus::Ok),
        metadata: if encoding_failed {
            vec![Attribute::bool(METADATA_CONTENT_ENCODE_FAILED, true)]
        } else {
            Vec::new()
        },
        output,
        error: error.map(observation_error),
        usage: Some(generation_usage(usage)),
        ..ObservationOutcome::default()
    });
}

pub(crate) fn finish_call(generation: Observation, error: Option<&LlmError>, usage: Option<&LlmTokenUsage>) {
    generation.finish(ObservationOutcome {
        status: error.map(observation_status).unwrap_or(ObservationStatus::Ok),
        error: error.map(observation_error),
        usage: usage.map(generation_usage),
        ..ObservationOutcome::default()
    });
}

pub(crate) struct CallMetadata<'a> {
    pub call_id: &'a str,
    pub queue_wait_ms: u64,
    pub provider_latency_ms: u64,
    pub total_latency_ms: u64,
    pub usage_accuracy: &'a str,
    pub reasoning_content_available: Option<bool>,
    pub finish_reason: Option<&'a str>,
}

pub(crate) fn record_call_metadata(generation: &mut Observation, metadata: CallMetadata<'_>) {
    generation.record_attribute(Attribute::string(METADATA_CALL_ID, metadata.call_id));
    generation.record_attribute(Attribute::u64(METADATA_QUEUE_WAIT_MS, metadata.queue_wait_ms));
    generation.record_attribute(Attribute::u64(METADATA_PROVIDER_LATENCY_MS, metadata.provider_latency_ms));
    generation.record_attribute(Attribute::u64(METADATA_TOTAL_LATENCY_MS, metadata.total_latency_ms));
    generation.record_attribute(Attribute::string(METADATA_USAGE_ACCURACY, metadata.usage_accuracy));
    if let Some(available) = metadata.reasoning_content_available {
        generation.record_attribute(Attribute::bool(METADATA_REASONING_CONTENT_AVAILABLE, available));
    }
    if let Some(reason) = metadata.finish_reason {
        generation.record_attribute(Attribute::string(METADATA_FINISH_REASON, reason));
    }
}

pub(crate) fn generation_usage(usage: &LlmTokenUsage) -> GenerationUsage {
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

pub(crate) fn observation_status(error: &LlmError) -> ObservationStatus {
    match error {
        LlmError::Cancelled => ObservationStatus::Cancelled,
        LlmError::TurnDeadlineExceeded | LlmError::ProviderTimeout => ObservationStatus::DeadlineExceeded,
        _ => ObservationStatus::Error,
    }
}

pub(crate) fn observation_error(error: &LlmError) -> ObservationError {
    ObservationError {
        code: error.kind().to_owned(),
        failure_kind: "llm".into(),
        stage: None,
        message: error.to_string(),
    }
}
