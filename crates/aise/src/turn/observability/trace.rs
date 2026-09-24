use super::fields::{
    BoundedContent, BoundedContentEncoder, ContentCaptureLimits, ContentCapturePolicy, ObservationAttribute,
    ObservationFields, ObservationFinish, ObservationStatus, ObservationValue, SCHEMA_VERSION, SESSION_ID,
    TRACE_ENVIRONMENT, TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST, TRACE_METADATA_STORY_ID, TRACE_METADATA_TURN_NUMBER,
    TRACE_NAME, TRACE_RELEASE, TRACE_TAGS,
};
use super::span::ObservationSpan;
use super::step::ObservationStep;
use opentelemetry::{Context, KeyValue, baggage::BaggageExt};
use serde::Serialize;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub struct ObservationTrace {
    root: ObservationSpan,
    context: Context,
    finished: bool,
    baggage: Vec<KeyValue>,
    output_encoder: Option<BoundedContentEncoder>,
    remaining_content_bytes: usize,
}

impl ObservationTrace {
    pub fn begin(fields: ObservationFields) -> Self {
        Self::begin_inner(fields, None)
    }

    pub fn begin_with_content_capture(
        fields: ObservationFields,
        policy: ContentCapturePolicy,
        limits: ContentCaptureLimits,
    ) -> Self {
        let remaining_content_bytes = limits
            .max_observation_bytes
            .saturating_sub(fields.input.as_ref().map_or(0, |input| input.captured_bytes));
        Self::begin_inner(
            fields,
            Some((BoundedContentEncoder::new(policy, limits), remaining_content_bytes)),
        )
    }

    fn begin_inner(fields: ObservationFields, content_capture: Option<(BoundedContentEncoder, usize)>) -> Self {
        let mut root = ObservationSpan::begin(ObservationStep::ExecuteStoryTurn, ObservationFields::default());
        let initial_baggage = if root.is_recording() {
            fields
                .metadata
                .iter()
                .filter_map(|attribute| {
                    if matches!(attribute.key, TRACE_ENVIRONMENT | TRACE_RELEASE) {
                        if let ObservationValue::String(value) = &attribute.value {
                            return Some((attribute.key, value.clone()));
                        }
                    }
                    None
                })
                .collect()
        } else {
            Vec::new()
        };
        root.record_fields(fields);
        let context = root.tracing_span().context();
        let (output_encoder, remaining_content_bytes) = match content_capture {
            Some((encoder, remaining)) => (Some(encoder), remaining),
            None => (None, 0),
        };
        let mut trace = Self {
            root,
            context,
            finished: false,
            baggage: Vec::new(),
            output_encoder,
            remaining_content_bytes,
        };
        if trace.root.is_recording() {
            trace.root.record_static(TRACE_NAME, ObservationStep::ExecuteStoryTurn.name());
            trace
                .root
                .record_attribute(ObservationAttribute::string_list(TRACE_TAGS, vec!["story-turn".to_owned()]));
            trace.bind_baggage(TRACE_NAME, ObservationStep::ExecuteStoryTurn.name().to_owned());
            trace.bind_baggage(TRACE_TAGS, "story-turn".to_owned());
            trace.bind_baggage(SCHEMA_VERSION, ObservationStep::SCHEMA_VERSION.to_owned());
            for (key, value) in initial_baggage {
                trace.bind_baggage(key, value);
            }
        }
        trace
    }

    pub(crate) fn encode_output<T: Serialize>(&self, value: &T) -> (Option<BoundedContent>, bool) {
        if !self.root.is_recording() {
            return (None, false);
        }
        self.output_encoder
            .as_ref()
            .map(|encoder| encoder.encode_with_status(value, self.remaining_content_bytes))
            .unwrap_or((None, false))
    }

    pub(crate) fn content_encoder(&self) -> Option<BoundedContentEncoder> {
        if self.root.is_recording() {
            self.output_encoder.clone()
        } else {
            None
        }
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn span(&self) -> Span {
        self.root.tracing_span()
    }

    pub fn bind_session(&mut self, session_id: &str, story_id: &str) {
        if !self.root.is_recording() {
            return;
        }
        self.root.record_attribute(ObservationAttribute::string(SESSION_ID, session_id));
        self.root
            .record_attribute(ObservationAttribute::string(TRACE_METADATA_STORY_ID, story_id));
        Span::current().set_attribute(SESSION_ID, session_id.to_owned());
        Span::current().set_attribute(TRACE_METADATA_STORY_ID, story_id.to_owned());
        self.bind_baggage(SESSION_ID, session_id.to_owned());
        self.bind_baggage(TRACE_METADATA_STORY_ID, story_id.to_owned());
    }

    pub fn bind_request(&mut self, idempotency_key_digest: &str) {
        if !self.root.is_recording() {
            return;
        }
        self.root.record_attribute(ObservationAttribute::string(
            TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST,
            idempotency_key_digest,
        ));
        Span::current().set_attribute(TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST, idempotency_key_digest.to_owned());
        self.bind_baggage(TRACE_METADATA_IDEMPOTENCY_KEY_DIGEST, idempotency_key_digest.to_owned());
    }

    pub fn bind_turn(&mut self, turn_number: u64) {
        if !self.root.is_recording() {
            return;
        }
        self.root
            .record_attribute(ObservationAttribute::u64(TRACE_METADATA_TURN_NUMBER, turn_number));
        if let Ok(turn_number) = i64::try_from(turn_number) {
            Span::current().set_attribute(TRACE_METADATA_TURN_NUMBER, turn_number);
        } else {
            Span::current().set_attribute(TRACE_METADATA_TURN_NUMBER, turn_number.to_string());
        }
        self.bind_baggage(TRACE_METADATA_TURN_NUMBER, turn_number.to_string());
    }

    pub fn finish(mut self, finish: ObservationFinish) {
        self.root.finish_inner(finish);
        self.finished = true;
    }

    fn bind_baggage(&mut self, key: &'static str, value: String) {
        if let Some(existing) = self.baggage.iter_mut().find(|existing| existing.key.as_str() == key) {
            *existing = KeyValue::new(key, value);
        } else {
            self.baggage.push(KeyValue::new(key, value));
        }
        self.context = self.context.with_baggage(self.baggage.clone());
    }
}

impl Drop for ObservationTrace {
    fn drop(&mut self) {
        if !self.finished {
            self.root.finish_inner(ObservationFinish {
                status: ObservationStatus::Incomplete,
                ..ObservationFinish::default()
            });
            self.finished = true;
        }
    }
}

#[cfg(test)]
#[path = "tests/trace_tests.rs"]
mod tests;
