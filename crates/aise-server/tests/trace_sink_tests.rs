use aise::turn::turn_trace::{TraceId, TraceSpan, TraceSpanSink, TurnTrace};
use aise_server::trace::{NoopRedactor, TraceRedactor, TraceSink, TraceWriter, TraceWriterConfig};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct SecretRedactor;

impl TraceRedactor for SecretRedactor {
    fn redact_value(&self, value: &mut serde_json::Value) {
        if let Some(object) = value.as_object_mut() {
            for child in object.values_mut() {
                if child.as_str().is_some_and(|s| s.contains("secret-value")) {
                    *child = serde_json::json!("REDACTED");
                }
            }
            for child in object.values_mut() {
                self.redact_value(child);
            }
        } else if let Some(items) = value.as_array_mut() {
            for child in items.iter_mut() {
                self.redact_value(child);
            }
        }
    }
}

fn sample_span(kind: &str, name: &str) -> TraceSpan {
    TraceSpan {
        span_id: format!("span-{name}"),
        parent_span_id: None,
        kind: kind.to_owned(),
        name: name.to_owned(),
        started_at_ms: 1,
        ended_at_ms: 2,
        duration_ms: 1,
        payload: serde_json::json!({ "ok": true }),
    }
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("aise_trace_{label}_{now}"))
}

fn config() -> TraceWriterConfig {
    TraceWriterConfig {
        channel_capacity: 16,
        max_record_bytes: 1024 * 1024,
        rotation_bytes: 0,
        retention_files: 0,
        shutdown_grace_ms: 2_000,
    }
}

#[tokio::test]
async fn writes_spans_as_jsonl_and_trace_as_json() {
    let dir = temp_dir("sink");
    let sink = TraceWriter::new(config(), dir.clone(), Arc::new(NoopRedactor)).unwrap();
    let trace_id = TraceId::new_id();

    sink.write_span(&trace_id, &sample_span("aise.pipeline", "planner"));
    sink.write_span(&trace_id, &sample_span("aise.validation", "schema"));

    let trace = TurnTrace {
        trace_id: trace_id.clone(),
        story_id: "story-1".to_owned(),
        turn_number: Some(aise::domain::TurnNumber::try_new(1).unwrap()),
        started_at_ms: 1,
        ended_at_ms: 2,
        duration_ms: 1,
        dropped_span_count: 0,
        spans: vec![
            sample_span("aise.pipeline", "planner"),
            sample_span("aise.validation", "schema"),
        ],
    };
    sink.write_trace(&trace);
    sink.shutdown_with_grace().await;

    let stem = trace_id.file_stem();
    assert_eq!(stem.len(), 23);
    assert!(stem.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_'));
    let jsonl = std::fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
    let lines: Vec<&str> = jsonl.lines().collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["kind"], "aise.pipeline");
    assert_eq!(first["name"], "planner");
    assert_eq!(first["payload"]["ok"], true);
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second["kind"], "aise.validation");

    let full = std::fs::read_to_string(dir.join(format!("{stem}.json"))).unwrap();
    let parsed: TurnTrace = serde_json::from_str(&full).unwrap();
    assert_eq!(parsed.trace_id, trace_id);
    assert_eq!(parsed.turn_number, Some(aise::domain::TurnNumber::try_new(1).unwrap()));
    assert_eq!(parsed.spans.len(), 2);
}

#[tokio::test]
async fn trace_writer_applies_bounded_backpressure() {
    let dir = temp_dir("backpressure");
    let config = TraceWriterConfig {
        channel_capacity: 1,
        max_record_bytes: 1024 * 1024,
        rotation_bytes: 0,
        retention_files: 0,
        shutdown_grace_ms: 2_000,
    };
    let sink = TraceWriter::new(config, dir.clone(), Arc::new(NoopRedactor)).unwrap();
    let trace_id = TraceId::try_new("11111111111111111111111111111111").unwrap();
    let span = sample_span("aise.pipeline", "planner");

    let mut rejected = 0;
    for _ in 0..16 {
        if sink
            .try_write(aise::turn::turn_trace::TraceRecord::Span {
                trace_id: trace_id.clone(),
                span: span.clone(),
            })
            .is_err()
        {
            rejected += 1;
        }
    }
    assert!(rejected > 0, "trace queue must apply bounded backpressure");
    sink.shutdown_with_grace().await;

    let jsonl = std::fs::read_to_string(dir.join(format!("{}.jsonl", trace_id.file_stem()))).unwrap();
    assert!(jsonl.lines().count() > 0, "accepted records were written");
}

#[tokio::test]
async fn trace_writer_rotates_and_enforces_retention() {
    let dir = temp_dir("rotation");
    let config = TraceWriterConfig {
        channel_capacity: 64,
        max_record_bytes: 1024 * 1024,
        rotation_bytes: 128,
        retention_files: 2,
        shutdown_grace_ms: 2_000,
    };
    let sink = TraceWriter::new(config, dir.clone(), Arc::new(NoopRedactor)).unwrap();
    let trace_id = TraceId::try_new("22222222222222222222222222222222").unwrap();
    let span = sample_span("aise.pipeline", "planner");

    for _ in 0..12 {
        let _ = sink.try_write(aise::turn::turn_trace::TraceRecord::Span {
            trace_id: trace_id.clone(),
            span: span.clone(),
        });
    }
    sink.shutdown_with_grace().await;

    let file_count = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .count();
    assert!(file_count <= 2, "retention leaves at most configured files, got {file_count}");
}

#[tokio::test]
async fn shutdown_drains_trace_writer_within_grace() {
    let dir = temp_dir("drain");
    let sink = TraceWriter::new(config(), dir.clone(), Arc::new(NoopRedactor)).unwrap();
    let trace_id = TraceId::try_new("33333333333333333333333333333333").unwrap();
    let span = sample_span("aise.pipeline", "planner");

    for _ in 0..8 {
        let _ = sink.try_write(aise::turn::turn_trace::TraceRecord::Span {
            trace_id: trace_id.clone(),
            span: span.clone(),
        });
    }
    let started = SystemTime::now();
    sink.shutdown_with_grace().await;
    let elapsed = started.elapsed().unwrap();
    assert!(elapsed < Duration::from_secs(2), "drain completes within grace");

    let jsonl = std::fs::read_to_string(dir.join(format!("{}.jsonl", trace_id.file_stem()))).unwrap();
    assert_eq!(jsonl.lines().count(), 8, "all accepted records drained on shutdown");
}

#[tokio::test]
async fn content_trace_redacts_before_truncation() {
    let dir = temp_dir("redact_order");
    let config = TraceWriterConfig {
        channel_capacity: 16,
        max_record_bytes: 128,
        rotation_bytes: 0,
        retention_files: 0,
        shutdown_grace_ms: 2_000,
    };
    let sink = TraceWriter::new(config, dir.clone(), Arc::new(SecretRedactor)).unwrap();
    let trace_id = TraceId::try_new("44444444444444444444444444444444").unwrap();
    let span = TraceSpan {
        span_id: "span-redact".into(),
        parent_span_id: None,
        kind: "aise.pipeline".into(),
        name: "planner".into(),
        started_at_ms: 1,
        ended_at_ms: 2,
        duration_ms: 1,
        payload: serde_json::json!({
            "aaa_secret": format!("secret-value-{}", "x".repeat(512)),
            "zzz_pad": "x".repeat(512),
        }),
    };
    sink.write_span(&trace_id, &span);
    sink.shutdown_with_grace().await;

    let jsonl = std::fs::read_to_string(dir.join(format!("{}.jsonl", trace_id.file_stem()))).unwrap();
    assert!(
        jsonl.contains("REDACTED"),
        "redactor replaced the secret before byte truncation"
    );
    assert!(
        !jsonl.contains("secret-value"),
        "raw secret never reaches disk, redaction precedes truncation"
    );
    assert!(jsonl.len() <= 128 + 1, "record is truncated to max_record_bytes plus newline");
}
