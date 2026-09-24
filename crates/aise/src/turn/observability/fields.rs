use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

pub const OBSERVATION_TYPE: &str = "aise.observation.type";
pub const OBSERVATION_INPUT: &str = "aise.observation.input";
pub const OBSERVATION_OUTPUT: &str = "aise.observation.output";
pub const OBSERVATION_MODEL_NAME: &str = "aise.observation.model.name";
pub const OBSERVATION_MODEL_PARAMETERS: &str = "aise.observation.model.parameters";
pub const OBSERVATION_USAGE_DETAILS: &str = "aise.observation.usage_details";
pub const OBSERVATION_COST_DETAILS: &str = "aise.observation.cost_details";
pub const OBSERVATION_LEVEL: &str = "aise.observation.level";
pub const OBSERVATION_STATUS_MESSAGE: &str = "aise.observation.status_message";
pub const TRACE_NAME: &str = "aise.trace.name";
pub const TRACE_TAGS: &str = "aise.trace.tags";
pub const TRACE_ENVIRONMENT: &str = "aise.trace.environment";
pub const TRACE_RELEASE: &str = "aise.trace.release";
pub const SCHEMA_VERSION: &str = "aise.schema.version";
pub const SESSION_ID: &str = "langfuse.session.id";
pub const TRACE_METADATA_STORY_ID: &str = "aise.trace.metadata.story_id";
pub const TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST: &str = "aise.trace.metadata.idempotency_key_digest";
pub const TRACE_METADATA_TURN_NUMBER: &str = "aise.trace.metadata.turn_number";
pub const METADATA_REPLAYED: &str = "aise.observation.metadata.replayed";
pub const METADATA_TERMINAL_STATUS: &str = "aise.observation.metadata.terminal_status";
pub const METADATA_FAILURE_STAGE: &str = "aise.observation.metadata.failure_stage";
pub const METADATA_STORY_ID: &str = "aise.observation.metadata.story_id";
pub const METADATA_TURN_NUMBER: &str = "aise.observation.metadata.turn_number";
pub const METADATA_RETRIEVAL_SKIPPED: &str = "aise.observation.metadata.retrieval_skipped";
pub const METADATA_CHARACTER_THINKING_SKIPPED: &str = "aise.observation.metadata.character_thinking_skipped";
pub const METADATA_SKIP_REASON: &str = "aise.observation.metadata.skip_reason";
pub const METADATA_SNAPSHOT_REVISION: &str = "aise.observation.metadata.snapshot_revision";
pub const METADATA_RETURNED_ITEM_COUNT: &str = "aise.observation.metadata.returned_item_count";
pub const METADATA_REQUEST_COUNT: &str = "aise.observation.metadata.request_count";
pub const METADATA_CANDIDATE_COUNT: &str = "aise.observation.metadata.candidate_count";
pub const METADATA_ACTIVATED_COUNT: &str = "aise.observation.metadata.activated_count";
pub const METADATA_GRAPH_REVISION: &str = "aise.observation.metadata.graph_revision";
pub const METADATA_PROJECTED_NODE_COUNT: &str = "aise.observation.metadata.projected_node_count";
pub const METADATA_PROJECTED_EDGE_COUNT: &str = "aise.observation.metadata.projected_edge_count";
pub const METADATA_CHARACTER_ID: &str = "aise.observation.metadata.character_id";
pub const METADATA_PROVIDER: &str = "aise.observation.metadata.provider";
pub const METADATA_CALL_ID: &str = "aise.observation.metadata.call_id";
pub const METADATA_ATTEMPT: &str = "aise.observation.metadata.attempt";
pub const METADATA_CORRECTION_ROUND: &str = "aise.observation.metadata.correction_round";
pub const METADATA_QUEUE_WAIT_MS: &str = "aise.observation.metadata.queue_wait_ms";
pub const METADATA_PROVIDER_LATENCY_MS: &str = "aise.observation.metadata.provider_latency_ms";
pub const METADATA_TOTAL_LATENCY_MS: &str = "aise.observation.metadata.total_latency_ms";
pub const METADATA_USAGE_ACCURACY: &str = "aise.observation.metadata.usage_accuracy";
pub const METADATA_FINISH_REASON: &str = "aise.observation.metadata.finish_reason";
pub const METADATA_REASONING_CONTENT_AVAILABLE: &str = "aise.observation.metadata.reasoning_content_available";
pub const METADATA_COMMIT_STATUS: &str = "aise.observation.metadata.commit_status";
pub const METADATA_ERROR_CODE: &str = "aise.observation.metadata.error_code";
pub const METADATA_FAILURE_KIND: &str = "aise.observation.metadata.failure_kind";
pub const METADATA_STAGE: &str = "aise.observation.metadata.stage";
pub const METADATA_CONTENT_ENCODE_FAILED: &str = "aise.observation.metadata.content_encode_failed";
pub const METADATA_INPUT_ORIGINAL_BYTES: &str = "aise.observation.metadata.input_original_bytes";
pub const METADATA_INPUT_CAPTURED_BYTES: &str = "aise.observation.metadata.input_captured_bytes";
pub const METADATA_INPUT_TRUNCATED: &str = "aise.observation.metadata.input_truncated";
pub const METADATA_INPUT_SHA256: &str = "aise.observation.metadata.input_sha256";
pub const METADATA_OUTPUT_ORIGINAL_BYTES: &str = "aise.observation.metadata.output_original_bytes";
pub const METADATA_OUTPUT_CAPTURED_BYTES: &str = "aise.observation.metadata.output_captured_bytes";
pub const METADATA_OUTPUT_TRUNCATED: &str = "aise.observation.metadata.output_truncated";
pub const METADATA_OUTPUT_SHA256: &str = "aise.observation.metadata.output_sha256";

pub fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObservationStatus {
    Ok,
    Error,
    Cancelled,
    DeadlineExceeded,
    Conflict,
    #[default]
    Incomplete,
}

impl ObservationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::Conflict => "conflict",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentCapturePolicy {
    MetadataOnly,
    RedactedContent,
    FullContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub failure_kind: String,
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationFields {
    pub metadata: Vec<ObservationAttribute>,
    pub input: Option<BoundedContent>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationFinish {
    pub status: ObservationStatus,
    pub metadata: Vec<ObservationAttribute>,
    pub output: Option<BoundedContent>,
    pub error: Option<ObservationError>,
    pub usage: Option<GenerationUsage>,
    pub cost: Option<GenerationCost>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationAttribute {
    pub key: &'static str,
    pub value: ObservationValue,
}

impl ObservationAttribute {
    pub fn string(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: ObservationValue::String(value.into()),
        }
    }

    pub fn optional_string(key: &'static str, value: Option<impl Into<String>>) -> Option<Self> {
        value.map(|value| Self::string(key, value))
    }

    pub const fn bool(key: &'static str, value: bool) -> Self {
        Self {
            key,
            value: ObservationValue::Bool(value),
        }
    }

    pub const fn i64(key: &'static str, value: i64) -> Self {
        Self {
            key,
            value: ObservationValue::I64(value),
        }
    }

    pub const fn u64(key: &'static str, value: u64) -> Self {
        Self {
            key,
            value: ObservationValue::U64(value),
        }
    }

    pub const fn f64(key: &'static str, value: f64) -> Self {
        Self {
            key,
            value: ObservationValue::F64(value),
        }
    }

    pub fn string_list(key: &'static str, value: Vec<String>) -> Self {
        Self {
            key,
            value: ObservationValue::StringList(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObservationValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StringList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenerationUsage {
    pub input: u64,
    pub input_cached_tokens: u64,
    pub output: u64,
    pub output_reasoning_tokens: u64,
    pub total: u64,
}

impl GenerationUsage {
    pub fn is_valid(&self) -> bool {
        self.input
            .checked_add(self.input_cached_tokens)
            .and_then(|total| total.checked_add(self.output))
            .and_then(|total| total.checked_add(self.output_reasoning_tokens))
            == Some(self.total)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenerationCost {
    pub currency: &'static str,
    pub input: Option<f64>,
    pub input_cached_tokens: Option<f64>,
    pub output: Option<f64>,
    pub output_reasoning_tokens: Option<f64>,
    pub total: f64,
}

impl GenerationCost {
    pub fn is_exportable(&self) -> bool {
        self.currency == "USD"
            && self.total.is_finite()
            && self.total >= 0.0
            && [
                self.input,
                self.input_cached_tokens,
                self.output,
                self.output_reasoning_tokens,
            ]
            .into_iter()
            .flatten()
            .all(|value| value.is_finite() && value >= 0.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCaptureLimits {
    pub max_field_bytes: usize,
    pub max_observation_bytes: usize,
    pub detector_overlap_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedContent {
    pub json: String,
    pub original_bytes: usize,
    pub captured_bytes: usize,
    pub truncated: bool,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationCaptureConfig {
    pub policy: ContentCapturePolicy,
    pub limits: ContentCaptureLimits,
}

impl ObservationCaptureConfig {
    pub const fn new(policy: ContentCapturePolicy, limits: ContentCaptureLimits) -> Self {
        Self { policy, limits }
    }

    pub const fn metadata_only() -> Self {
        Self {
            policy: ContentCapturePolicy::MetadataOnly,
            limits: ContentCaptureLimits {
                max_field_bytes: 16_384,
                max_observation_bytes: 32_768,
                detector_overlap_bytes: 512,
            },
        }
    }
}

#[derive(Clone)]
pub struct BoundedContentEncoder {
    policy: ContentCapturePolicy,
    limits: ContentCaptureLimits,
}

impl BoundedContentEncoder {
    pub fn new(policy: ContentCapturePolicy, limits: ContentCaptureLimits) -> Self {
        Self { policy, limits }
    }

    pub fn encode<T: Serialize>(&self, value: &T, remaining_observation_bytes: usize) -> Option<BoundedContent> {
        self.encode_with_status(value, remaining_observation_bytes).0
    }

    pub fn encode_with_status<T: Serialize>(
        &self,
        value: &T,
        remaining_observation_bytes: usize,
    ) -> (Option<BoundedContent>, bool) {
        if self.policy == ContentCapturePolicy::MetadataOnly {
            return (None, false);
        }

        let field_limit = self.limits.max_field_bytes.saturating_add(self.limits.detector_overlap_bytes);
        let observation_limit = remaining_observation_bytes
            .min(self.limits.max_observation_bytes)
            .saturating_add(self.limits.detector_overlap_bytes);
        let mut writer = BoundedHashWriter::new(field_limit.min(observation_limit));
        match serde_json::to_writer(&mut writer, value) {
            Ok(()) => (Some(writer.finish()), false),
            Err(_) => (None, true),
        }
    }
}

pub struct ObservationContentCapture {
    encoder: BoundedContentEncoder,
    remaining_bytes: usize,
}

impl ObservationContentCapture {
    pub fn new(config: ObservationCaptureConfig) -> Self {
        let remaining_bytes = config.limits.max_observation_bytes;
        Self {
            encoder: BoundedContentEncoder::new(config.policy, config.limits),
            remaining_bytes,
        }
    }

    pub fn capture<T: Serialize>(&mut self, value: &T) -> (Option<BoundedContent>, bool) {
        let (content, failed) = self.encoder.encode_with_status(value, self.remaining_bytes);
        if let Some(content) = &content {
            self.remaining_bytes = self.remaining_bytes.saturating_sub(content.captured_bytes);
        }
        (content, failed)
    }

    pub fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }
}

struct BoundedHashWriter {
    captured: Vec<u8>,
    limit: usize,
    original_bytes: usize,
    hasher: Sha256,
}

impl BoundedHashWriter {
    fn new(limit: usize) -> Self {
        Self {
            captured: Vec::new(),
            limit,
            original_bytes: 0,
            hasher: Sha256::new(),
        }
    }

    fn finish(mut self) -> BoundedContent {
        while std::str::from_utf8(&self.captured).is_err() {
            self.captured.pop();
        }
        let captured_bytes = self.captured.len();
        let json = String::from_utf8(self.captured).expect("captured JSON prefix is valid UTF-8");
        BoundedContent {
            json,
            original_bytes: self.original_bytes,
            captured_bytes,
            truncated: self.original_bytes > captured_bytes,
            sha256: format!("{:x}", self.hasher.finalize()),
        }
    }
}

impl Write for BoundedHashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.update(bytes);
        self.original_bytes = self.original_bytes.saturating_add(bytes.len());
        let remaining = self.limit.saturating_sub(self.captured.len());
        self.captured.extend_from_slice(&bytes[..remaining.min(bytes.len())]);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/fields_tests.rs"]
mod tests;
