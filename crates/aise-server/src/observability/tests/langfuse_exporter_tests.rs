use super::*;
use opentelemetry::trace::{SpanContext, SpanId, SpanKind, TraceFlags, TraceId, TraceState};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SpanEvents, SpanLinks};
use std::borrow::Cow;
use std::time::SystemTime;

#[derive(Debug)]
struct NoopExporter;

impl SpanExporter for NoopExporter {
    async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
        Ok(())
    }
}

fn span(attributes: Vec<KeyValue>) -> SpanData {
    SpanData {
        span_context: SpanContext::new(
            TraceId::from(1),
            SpanId::from(1),
            TraceFlags::SAMPLED,
            false,
            TraceState::default(),
        ),
        parent_span_id: SpanId::INVALID,
        parent_span_is_remote: false,
        span_kind: SpanKind::Internal,
        name: Cow::Borrowed("test"),
        start_time: SystemTime::UNIX_EPOCH,
        end_time: SystemTime::UNIX_EPOCH,
        attributes,
        dropped_attributes_count: 0,
        events: SpanEvents::default(),
        links: SpanLinks::default(),
        status: Status::Unset,
        instrumentation_scope: Default::default(),
    }
}

fn mapped_keys(span: &SpanData) -> Vec<&str> {
    span.attributes.iter().map(|attribute| attribute.key.as_str()).collect()
}

#[test]
fn maps_every_known_temporary_key() {
    let source_keys = [
        "aise.observation.type",
        "aise.observation.input",
        "aise.observation.output",
        "aise.observation.model.name",
        "aise.observation.model.parameters",
        "aise.observation.usage_details",
        "aise.observation.cost_details",
        "aise.observation.level",
        "aise.observation.status_message",
        "aise.trace.name",
        "aise.trace.tags",
        "aise.trace.environment",
        "aise.trace.release",
        "aise.schema.version",
        "aise.observation.metadata.error_code",
        "aise.trace.metadata.story_id",
    ];
    let mut batch = vec![span(
        source_keys.into_iter().map(|key| KeyValue::new(key, "value")).collect(),
    )];
    let adapter = LangfuseExportAdapter::new(NoopExporter, StreamingMasker::new(1024, false), TelemetryDiagnostics);

    adapter.map_batch(&mut batch);

    let keys = mapped_keys(&batch[0]);
    assert!(keys.contains(&"langfuse.observation.type"));
    assert!(keys.contains(&"langfuse.observation.input"));
    assert!(keys.contains(&"langfuse.observation.output"));
    assert!(keys.contains(&"langfuse.observation.model.name"));
    assert!(keys.contains(&"langfuse.observation.model.parameters"));
    assert!(keys.contains(&"langfuse.observation.usage_details"));
    assert!(keys.contains(&"langfuse.observation.cost_details"));
    assert!(keys.contains(&"langfuse.observation.level"));
    assert!(keys.contains(&"langfuse.observation.status_message"));
    assert!(keys.contains(&"langfuse.trace.name"));
    assert!(keys.contains(&"langfuse.trace.tags"));
    assert!(keys.contains(&"langfuse.environment"));
    assert!(keys.contains(&"langfuse.release"));
    assert!(keys.contains(&"langfuse.version"));
    assert!(keys.contains(&"langfuse.observation.metadata.error_code"));
    assert!(keys.contains(&"langfuse.trace.metadata.story_id"));
    assert!(!keys.iter().any(|key| key.starts_with("aise.")));
}

#[test]
fn removes_unknown_aise_attributes_and_preserves_other_attributes() {
    let mut batch = vec![span(vec![
        KeyValue::new("aise.unknown", "drop"),
        KeyValue::new("http.request.method", "POST"),
    ])];
    let adapter = LangfuseExportAdapter::new(NoopExporter, StreamingMasker::new(1024, false), TelemetryDiagnostics);

    adapter.map_batch(&mut batch);

    assert_eq!(mapped_keys(&batch[0]), ["http.request.method"]);
}

#[test]
fn masks_and_then_truncates_every_exported_string_attribute() {
    let mut batch = vec![span(vec![
        KeyValue::new("aise.observation.output", "prefix Authorization: Bearer secret-token suffix"),
        KeyValue::new("custom.array", Value::Array(Array::String(vec!["api_key=hidden-value".into()]))),
    ])];
    batch[0].status = Status::error("password=hunter2");
    let adapter = LangfuseExportAdapter::new(NoopExporter, StreamingMasker::new(32, false), TelemetryDiagnostics);

    adapter.map_batch(&mut batch);

    for attribute in &batch[0].attributes {
        assert!(!attribute.value.to_string().contains("secret-token"));
        assert!(!attribute.value.to_string().contains("hidden-value"));
        assert!(attribute.value.to_string().len() <= 34);
    }
    assert!(!format!("{:?}", batch[0].status).contains("hunter2"));
}
