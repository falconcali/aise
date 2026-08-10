use crate::config::TraceContentPolicy;
use crate::domain::ids::{StoryId, TurnId};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_LLM_CONTENT_CHARS: usize = 2000;
pub const MAX_LLM_RESPONSE_CHARS: usize = 4000;

pub fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    let prefix: String = text.chars().take(max_chars).collect();
    format!("{prefix}…[+{} chars]", count - max_chars)
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TraceIdError {
    #[error("trace_id must not be empty")]
    EmptyTraceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(String);

impl TraceId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TraceIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TraceIdError::EmptyTraceId);
        }
        Ok(Self(value))
    }

    pub fn new_id() -> Self {
        Self(format!("{}-{}", local_stamp(), Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_stem(&self) -> &str {
        match self.0.rsplit_once('-') {
            Some((stamp, _)) => stamp,
            None => &self.0,
        }
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTrace {
    pub trace_id: TraceId,
    pub turn_id: String,
    pub story_id: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub dropped_span_count: u32,
    pub spans: Vec<TraceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpanPayload {
    Turn(TurnData),
    Pipeline(PipelineData),
    LlmCall(Box<LlmCallData>),
    ToolCall(ToolCallData),
    Validation(ValidationData),
    Persist(PersistData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnData {
    pub story_id: String,
    pub turn_id: String,
    pub player_input: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineData {
    pub stage: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallData {
    pub provider: String,
    pub model: String,
    pub purpose: String,
    pub stream: bool,
    pub attempt: u32,
    pub queue_wait_ms: u64,
    pub provider_latency_ms: u64,
    pub total_latency_ms: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub reasoning_tokens: Option<u64>,
    pub usage_accuracy: String,
    pub finish_reason: Option<String>,
    pub charge: Option<serde_json::Value>,
    pub status: String,
    pub error_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<LlmCallContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallContent {
    pub messages: Vec<MessageData>,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallData {
    pub tool: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
    pub ok: bool,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationData {
    pub pass: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistData {
    pub turn_id: String,
    pub status: String,
    pub error: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone)]
pub enum TraceRecord {
    Span { trace_id: TraceId, span: TraceSpan },
    Completed(TurnTrace),
}

pub trait TraceSpanSink: Send + Sync {
    fn write_span(&self, trace_id: &TraceId, span: &TraceSpan);
    fn write_trace(&self, trace: &TurnTrace);
}

const DEFAULT_MAX_SPANS: usize = 64;

pub struct TraceRecorder {
    trace_id: TraceId,
    started_at_ms: u64,
    spans: Vec<TraceSpan>,
    current_parent: Option<String>,
    max_spans: usize,
    dropped: u32,
    span_sink: Option<Arc<dyn TraceSpanSink>>,
    content_policy: TraceContentPolicy,
}

impl TraceRecorder {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_SPANS)
    }

    pub fn with_limits(max_spans: usize) -> Self {
        Self {
            trace_id: TraceId::new_id(),
            started_at_ms: now_millis(),
            spans: Vec::new(),
            current_parent: None,
            max_spans,
            dropped: 0,
            span_sink: None,
            content_policy: TraceContentPolicy::MetadataOnly,
        }
    }

    pub fn with_sink(mut self, sink: Arc<dyn TraceSpanSink>) -> Self {
        self.span_sink = Some(sink);
        self
    }

    pub fn with_content_policy(mut self, policy: TraceContentPolicy) -> Self {
        self.content_policy = policy;
        self
    }

    pub fn content_policy(&self) -> TraceContentPolicy {
        self.content_policy
    }

    pub fn trace_id(&self) -> &TraceId {
        &self.trace_id
    }

    pub fn begin_span(&mut self, kind: &str, name: &str) -> PendingSpan {
        let span_id = new_id();
        let parent_span_id = self.current_parent.clone();
        self.current_parent = Some(span_id.clone());
        PendingSpan {
            span_id,
            parent_span_id,
            kind: kind.to_owned(),
            name: name.to_owned(),
            started_at_ms: now_millis(),
        }
    }

    pub fn end_span_with<S: Serialize>(&mut self, span: PendingSpan, payload: &S) {
        if self.current_parent.as_deref() == Some(span.span_id.as_str()) {
            self.current_parent = span.parent_span_id.clone();
        }
        let ended_at_ms = now_millis();
        let trace_span = TraceSpan {
            span_id: span.span_id,
            parent_span_id: span.parent_span_id,
            kind: span.kind,
            name: span.name,
            started_at_ms: span.started_at_ms,
            ended_at_ms,
            duration_ms: ended_at_ms.saturating_sub(span.started_at_ms),
            payload: to_value(payload),
        };
        if let Some(sink) = &self.span_sink {
            sink.write_span(&self.trace_id, &trace_span);
        }
        tracing::info!(
            trace_id = self.trace_id.as_str(),
            kind = trace_span.kind.as_str(),
            name = trace_span.name.as_str(),
            duration_ms = trace_span.duration_ms,
            "aise.trace.span"
        );
        self.push_span(trace_span);
    }

    pub fn record_span<S: Serialize>(&mut self, kind: &str, name: &str, payload: &S) {
        let at_ms = now_millis();
        let trace_span = TraceSpan {
            span_id: new_id(),
            parent_span_id: self.current_parent.clone(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            started_at_ms: at_ms,
            ended_at_ms: at_ms,
            duration_ms: 0,
            payload: to_value(payload),
        };
        if let Some(sink) = &self.span_sink {
            sink.write_span(&self.trace_id, &trace_span);
        }
        tracing::info!(
            trace_id = self.trace_id.as_str(),
            kind = trace_span.kind.as_str(),
            name = trace_span.name.as_str(),
            "aise.trace.span"
        );
        self.push_span(trace_span);
    }

    pub fn build(&mut self, story_id: &StoryId, turn_id: &TurnId) -> TurnTrace {
        let ended_at_ms = now_millis();
        TurnTrace {
            trace_id: self.trace_id.clone(),
            turn_id: turn_id.to_string(),
            story_id: story_id.to_string(),
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            duration_ms: ended_at_ms.saturating_sub(self.started_at_ms),
            dropped_span_count: self.dropped,
            spans: std::mem::take(&mut self.spans),
        }
    }

    fn push_span(&mut self, span: TraceSpan) {
        if self.spans.len() >= self.max_spans {
            self.dropped += 1;
            return;
        }
        self.spans.push(span);
    }
}

impl Default for TraceRecorder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PendingSpan {
    span_id: String,
    parent_span_id: Option<String>,
    kind: String,
    name: String,
    started_at_ms: u64,
}

fn to_value<S: Serialize>(payload: &S) -> serde_json::Value {
    match serde_json::to_value(payload) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(event = "aise.trace.payload_serialize_failed", error = %error);
            serde_json::Value::Null
        }
    }
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn local_stamp() -> String {
    Local::now().format("%Y-%m-%d-%H_%M_%S_%3f").to_string()
}
