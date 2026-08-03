use crate::domain::ids::{StoryId, TurnId};
use crate::runtime::trace::span::{TraceSpan, TurnTrace};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DEFAULT_MAX_SPANS: usize = 64;

pub struct TraceRecorder {
    trace_id: String,
    started_at_ms: u64,
    spans: Vec<TraceSpan>,
    current_parent: Option<String>,
    max_spans: usize,
    dropped: u32,
}

impl TraceRecorder {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_SPANS)
    }

    pub fn with_limits(max_spans: usize) -> Self {
        Self {
            trace_id: new_id(),
            started_at_ms: now_millis(),
            spans: Vec::new(),
            current_parent: None,
            max_spans,
            dropped: 0,
        }
    }

    pub fn trace_id(&self) -> &str {
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

#[cfg(test)]
#[path = "tests/recorder_tests.rs"]
mod tests;
