use serde::Serialize;

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
pub const METADATA_CONTENT_ENCODE_FAILED: &str = "aise.observation.metadata.content_encode_failed";
pub const METADATA_INPUT_ORIGINAL_BYTES: &str = "aise.observation.metadata.input_original_bytes";
pub const METADATA_INPUT_CAPTURED_BYTES: &str = "aise.observation.metadata.input_captured_bytes";
pub const METADATA_INPUT_TRUNCATED: &str = "aise.observation.metadata.input_truncated";
pub const METADATA_INPUT_SHA256: &str = "aise.observation.metadata.input_sha256";
pub const METADATA_OUTPUT_ORIGINAL_BYTES: &str = "aise.observation.metadata.output_original_bytes";
pub const METADATA_OUTPUT_CAPTURED_BYTES: &str = "aise.observation.metadata.output_captured_bytes";
pub const METADATA_OUTPUT_TRUNCATED: &str = "aise.observation.metadata.output_truncated";
pub const METADATA_OUTPUT_SHA256: &str = "aise.observation.metadata.output_sha256";
pub const METADATA_ERROR_CODE: &str = "aise.observation.metadata.error_code";
pub const METADATA_FAILURE_KIND: &str = "aise.observation.metadata.failure_kind";
pub const METADATA_STAGE: &str = "aise.observation.metadata.stage";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationKind {
    Chain,
    Span,
    Generation,
    Retriever,
    Tool,
    Evaluator,
}

impl ObservationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Span => "span",
            Self::Generation => "generation",
            Self::Retriever => "retriever",
            Self::Tool => "tool",
            Self::Evaluator => "evaluator",
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StringList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub key: &'static str,
    pub value: AttributeValue,
}

impl Attribute {
    pub fn string(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: AttributeValue::String(value.into()),
        }
    }

    pub const fn bool(key: &'static str, value: bool) -> Self {
        Self {
            key,
            value: AttributeValue::Bool(value),
        }
    }

    pub const fn i64(key: &'static str, value: i64) -> Self {
        Self {
            key,
            value: AttributeValue::I64(value),
        }
    }

    pub const fn u64(key: &'static str, value: u64) -> Self {
        Self {
            key,
            value: AttributeValue::U64(value),
        }
    }

    pub const fn f64(key: &'static str, value: f64) -> Self {
        Self {
            key,
            value: AttributeValue::F64(value),
        }
    }

    pub fn string_list(key: &'static str, value: Vec<String>) -> Self {
        Self {
            key,
            value: AttributeValue::StringList(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub failure_kind: String,
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
            .and_then(|value| value.checked_add(self.output))
            .and_then(|value| value.checked_add(self.output_reasoning_tokens))
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
pub enum ContentCapturePolicy {
    MetadataOnly,
    RedactedContent,
    FullContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityContentConfig {
    pub policy: ContentCapturePolicy,
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

#[derive(Debug, Clone)]
pub struct SessionSpec {
    pub id: Option<String>,
    pub user_id: Option<String>,
    pub metadata: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub struct TraceSpec {
    pub name: &'static str,
    pub input: Option<BoundedContent>,
    pub metadata: Vec<Attribute>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ObservationSpec {
    pub name: &'static str,
    pub kind: ObservationKind,
    pub input: Option<BoundedContent>,
    pub metadata: Vec<Attribute>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationOutcome {
    pub status: ObservationStatus,
    pub metadata: Vec<Attribute>,
    pub output: Option<BoundedContent>,
    pub error: Option<ObservationError>,
    pub usage: Option<GenerationUsage>,
    pub cost: Option<GenerationCost>,
}

pub type TraceOutcome = ObservationOutcome;

#[derive(Debug, Clone, Default)]
pub struct SessionOutcome {
    pub status: ObservationStatus,
    pub metadata: Vec<Attribute>,
}
