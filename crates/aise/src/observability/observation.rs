use super::content::ContentCapture;
use super::model::{
    Attribute, AttributeValue, BoundedContent, GenerationCost, GenerationUsage, ObservationError, ObservationOutcome,
    ObservationSpec, ObservationStatus,
};
use opentelemetry::{Array, Context, Value};
use serde::Serialize;
use tracing::{Span, field::Empty};
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub struct Observation {
    pub(crate) span: Span,
    pub(crate) context: Context,
    pub(crate) content: ContentCapture,
    finished: bool,
}

impl Observation {
    pub(crate) fn new(spec: ObservationSpec, parent: Option<&Context>, content: ContentCapture) -> Self {
        let span = tracing::span!(
            target: "aise::observation",
            parent: None,
            tracing::Level::INFO,
            "observation",
            "observation.name" = spec.name,
            "otel.status_code" = Empty,
            "otel.status_message" = Empty
        );
        if let Some(parent) = parent {
            let _ = span.set_parent(parent.clone());
        }
        let mut observation = Self {
            context: span.context(),
            span,
            content,
            finished: false,
        };
        observation.record_static(super::model::OBSERVATION_TYPE, spec.kind.as_str());
        observation.record_static(super::model::SCHEMA_VERSION, "2");
        observation.record_attributes(spec.metadata);
        if let Some(input) = spec.input {
            observation.record_content(input, ContentDirection::Input);
        }
        observation
    }

    pub fn begin(&self, spec: ObservationSpec) -> Observation {
        Self::new(spec, Some(&self.context), self.content.clone())
    }

    pub fn content_capture(&self) -> &ContentCapture {
        &self.content
    }

    pub fn capture_content<T: Serialize>(&self, value: &T) -> Option<BoundedContent> {
        self.content.encode(value, self.content.max_observation_bytes()).content
    }

    pub fn finish(mut self, outcome: ObservationOutcome) {
        self.finish_inner(outcome);
    }

    pub fn is_recording(&self) -> bool {
        !self.span.is_disabled()
    }

    pub(crate) fn record_attributes(&mut self, attributes: Vec<Attribute>) {
        for attribute in attributes {
            self.record_attribute(attribute);
        }
    }

    pub(crate) fn record_attribute(&mut self, attribute: Attribute) {
        if !self.is_recording() {
            return;
        }
        match attribute.value {
            AttributeValue::String(value) => self.span.set_attribute(attribute.key, value),
            AttributeValue::Bool(value) => self.span.set_attribute(attribute.key, value),
            AttributeValue::I64(value) => self.span.set_attribute(attribute.key, value),
            AttributeValue::U64(value) => {
                if let Ok(value) = i64::try_from(value) {
                    self.span.set_attribute(attribute.key, value);
                } else {
                    self.span.set_attribute(attribute.key, value.to_string());
                }
            }
            AttributeValue::F64(value) => self.span.set_attribute(attribute.key, value),
            AttributeValue::StringList(value) => self.span.set_attribute(
                attribute.key,
                Value::Array(Array::String(value.into_iter().map(Into::into).collect())),
            ),
        }
    }

    pub(crate) fn finish_incomplete(&mut self) {
        self.finish_inner(ObservationOutcome::default());
    }

    pub(crate) fn record_static(&mut self, key: &'static str, value: &'static str) {
        if self.is_recording() {
            self.span.set_attribute(key, value);
        }
    }

    fn finish_inner(&mut self, outcome: ObservationOutcome) {
        if self.finished {
            return;
        }
        if self.is_recording() {
            self.record_attributes(outcome.metadata);
            if let Some(output) = outcome.output {
                self.record_content(output, ContentDirection::Output);
            }
            if let Some(usage) = outcome.usage {
                self.record_usage(usage);
            }
            if let Some(cost) = outcome.cost {
                self.record_cost(cost);
            }
            self.record_status(outcome.status, outcome.error);
        }
        self.finished = true;
    }

    fn record_content(&mut self, content: BoundedContent, direction: ContentDirection) {
        let (content_key, original_key, captured_key, truncated_key, hash_key) = match direction {
            ContentDirection::Input => (
                super::model::OBSERVATION_INPUT,
                super::model::METADATA_INPUT_ORIGINAL_BYTES,
                super::model::METADATA_INPUT_CAPTURED_BYTES,
                super::model::METADATA_INPUT_TRUNCATED,
                super::model::METADATA_INPUT_SHA256,
            ),
            ContentDirection::Output => (
                super::model::OBSERVATION_OUTPUT,
                super::model::METADATA_OUTPUT_ORIGINAL_BYTES,
                super::model::METADATA_OUTPUT_CAPTURED_BYTES,
                super::model::METADATA_OUTPUT_TRUNCATED,
                super::model::METADATA_OUTPUT_SHA256,
            ),
        };
        self.span.set_attribute(content_key, content.json);
        self.record_attribute(Attribute::u64(original_key, content.original_bytes as u64));
        self.record_attribute(Attribute::u64(captured_key, content.captured_bytes as u64));
        self.record_attribute(Attribute::bool(truncated_key, content.truncated));
        self.span.set_attribute(hash_key, content.sha256);
    }

    fn record_usage(&mut self, usage: GenerationUsage) {
        if !usage.is_valid() {
            return;
        }
        if let Ok(value) = serde_json::to_string(&usage) {
            self.span.set_attribute(super::model::OBSERVATION_USAGE_DETAILS, value);
        } else {
            self.record_attribute(Attribute::bool(super::model::METADATA_CONTENT_ENCODE_FAILED, true));
        }
    }

    fn record_cost(&mut self, cost: GenerationCost) {
        if !cost.is_exportable() {
            return;
        }
        if let Ok(value) = serde_json::to_string(&cost) {
            self.span.set_attribute(super::model::OBSERVATION_COST_DETAILS, value);
        } else {
            self.record_attribute(Attribute::bool(super::model::METADATA_CONTENT_ENCODE_FAILED, true));
        }
    }

    fn record_status(&mut self, status: ObservationStatus, error: Option<ObservationError>) {
        match status {
            ObservationStatus::Ok => {
                self.span.record("otel.status_code", "OK");
            }
            ObservationStatus::Cancelled | ObservationStatus::Conflict | ObservationStatus::Incomplete => {
                self.span.record("otel.status_message", status.as_str());
                self.span.set_attribute(super::model::OBSERVATION_LEVEL, "WARNING");
                self.span
                    .set_attribute(super::model::OBSERVATION_STATUS_MESSAGE, status.as_str());
            }
            ObservationStatus::Error | ObservationStatus::DeadlineExceeded => {
                self.span.record("otel.status_code", "ERROR");
                self.span.record("otel.status_message", status.as_str());
                self.span.set_attribute(super::model::OBSERVATION_LEVEL, "ERROR");
                self.span
                    .set_attribute(super::model::OBSERVATION_STATUS_MESSAGE, status.as_str());
            }
        }
        if let Some(error) = error {
            let message = error.message.chars().take(1024).collect::<String>();
            self.span.record("otel.status_code", "ERROR");
            self.span.record("otel.status_message", message.as_str());
            self.span.set_attribute(
                super::model::OBSERVATION_LEVEL,
                if status == ObservationStatus::Conflict {
                    "WARNING"
                } else {
                    "ERROR"
                },
            );
            self.span.set_attribute(super::model::OBSERVATION_STATUS_MESSAGE, message);
            self.span.set_attribute(super::model::METADATA_ERROR_CODE, error.code);
            self.span.set_attribute(super::model::METADATA_FAILURE_KIND, error.failure_kind);
            if let Some(stage) = error.stage {
                self.span.set_attribute(super::model::METADATA_STAGE, stage);
            }
        }
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        if !self.finished {
            self.finish_inner(ObservationOutcome::default());
        }
    }
}

enum ContentDirection {
    Input,
    Output,
}

#[cfg(test)]
#[path = "tests/observation_tests.rs"]
mod tests;
