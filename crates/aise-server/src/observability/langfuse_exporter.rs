use crate::observability::diagnostics::TelemetryDiagnostics;
use crate::observability::propagation::StreamingMasker;
use opentelemetry::trace::Status;
use opentelemetry::{Array, KeyValue, Value};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SpanData, SpanExporter};
use std::borrow::Cow;
use std::time::Duration;

#[derive(Debug)]
pub struct LangfuseExportAdapter<E> {
    inner: E,
    masker: StreamingMasker,
    diagnostics: TelemetryDiagnostics,
    endpoint_host: String,
}

impl<E: SpanExporter> LangfuseExportAdapter<E> {
    pub fn new(inner: E, masker: StreamingMasker, diagnostics: TelemetryDiagnostics) -> Self {
        Self {
            inner,
            masker,
            diagnostics,
            endpoint_host: "unknown".into(),
        }
    }

    pub fn with_endpoint_host(mut self, endpoint_host: impl Into<String>) -> Self {
        self.endpoint_host = endpoint_host.into();
        self
    }

    pub fn map_batch(&self, batch: &mut [SpanData]) {
        for span in batch {
            map_span(span, &self.masker);
        }
    }
}

impl<E: SpanExporter> SpanExporter for LangfuseExportAdapter<E> {
    async fn export(&self, mut batch: Vec<SpanData>) -> OTelSdkResult {
        self.map_batch(&mut batch);
        let result = self.inner.export(batch).await;
        if result.is_err() {
            self.diagnostics.export("export_failed", None, &self.endpoint_host);
        }
        result
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &Resource) {
        let resource = Resource::builder_empty()
            .with_attributes(
                resource
                    .iter()
                    .filter_map(|(key, value)| map_attribute(KeyValue::new(key.clone(), value.clone()), &self.masker)),
            )
            .build();
        self.inner.set_resource(&resource);
    }
}

fn map_span(span: &mut SpanData, masker: &StreamingMasker) {
    if let Some(name) = observation_name(span) {
        span.name = Cow::Owned(masker.mask(&name));
    }
    span.attributes = span
        .attributes
        .drain(..)
        .filter_map(|attribute| map_attribute(attribute, masker))
        .collect();
    for event in &mut span.events.events {
        event.attributes = event
            .attributes
            .drain(..)
            .filter_map(|attribute| map_attribute(attribute, masker))
            .collect();
    }
    for link in &mut span.links.links {
        link.attributes = link
            .attributes
            .drain(..)
            .filter_map(|attribute| map_attribute(attribute, masker))
            .collect();
    }
    if let Status::Error { description } = &span.status {
        span.status = Status::error(masker.mask(description));
    }
}

fn observation_name(span: &SpanData) -> Option<String> {
    span.attributes.iter().find_map(|attribute| {
        if attribute.key.as_str() == "observation.name" {
            match &attribute.value {
                Value::String(value) if !value.as_ref().is_empty() => Some(value.as_ref().to_owned()),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn map_attribute(attribute: KeyValue, masker: &StreamingMasker) -> Option<KeyValue> {
    let source = attribute.key.as_str();
    let destination = map_key(source)?;
    let value = mask_value(attribute.value, masker);
    let value = if source == "aise.trace.tags" {
        match value {
            Value::String(value) => Value::Array(Array::String(vec![value])),
            value => value,
        }
    } else {
        value
    };
    Some(KeyValue::new(destination, value))
}

fn mask_value(value: Value, masker: &StreamingMasker) -> Value {
    match value {
        Value::String(value) => Value::String(masker.mask(value.as_str()).into()),
        Value::Array(Array::String(values)) => Value::Array(Array::String(
            values.into_iter().map(|value| masker.mask(value.as_str()).into()).collect(),
        )),
        value => value,
    }
}

fn map_key(key: &str) -> Option<String> {
    let exact = match key {
        "aise.observation.type" => Some("langfuse.observation.type"),
        "aise.observation.input" => Some("langfuse.observation.input"),
        "aise.observation.output" => Some("langfuse.observation.output"),
        "aise.observation.model.name" => Some("langfuse.observation.model.name"),
        "aise.observation.model.parameters" => Some("langfuse.observation.model.parameters"),
        "aise.observation.usage_details" => Some("langfuse.observation.usage_details"),
        "aise.observation.cost_details" => Some("langfuse.observation.cost_details"),
        "aise.observation.level" => Some("langfuse.observation.level"),
        "aise.observation.status_message" => Some("langfuse.observation.status_message"),
        "aise.trace.name" => Some("langfuse.trace.name"),
        "aise.trace.tags" => Some("langfuse.trace.tags"),
        "aise.trace.environment" => Some("langfuse.environment"),
        "aise.trace.release" => Some("langfuse.release"),
        "aise.schema.version" => Some("langfuse.version"),
        _ => None,
    };
    if let Some(exact) = exact {
        return Some(exact.into());
    }
    if let Some(suffix) = key.strip_prefix("aise.observation.metadata.") {
        return (!suffix.is_empty()).then(|| format!("langfuse.observation.metadata.{suffix}"));
    }
    if let Some(suffix) = key.strip_prefix("aise.trace.metadata.") {
        return (!suffix.is_empty()).then(|| format!("langfuse.trace.metadata.{suffix}"));
    }
    if key.starts_with("aise.") {
        None
    } else {
        Some(key.into())
    }
}

#[cfg(test)]
#[path = "tests/langfuse_exporter_tests.rs"]
mod tests;
