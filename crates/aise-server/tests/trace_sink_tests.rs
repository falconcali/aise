use aise::core::turn_trace::{TraceSpan, TraceSpanSink, TurnTrace};
use aise_server::trace::FileTraceSpanSink;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

#[test]
fn writes_spans_as_jsonl_and_trace_as_json() {
    let dir = temp_dir("sink");
    let sink = Arc::new(FileTraceSpanSink::new(dir.clone()));
    let trace_id = "abcdef0123456789abcdef0123456789";

    sink.write_span(trace_id, &sample_span("aise.pipeline", "planner"));
    sink.write_span(trace_id, &sample_span("aise.validation", "schema"));

    let trace = TurnTrace {
        trace_id: trace_id.to_owned(),
        turn_id: "turn-1".to_owned(),
        story_id: "story-1".to_owned(),
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

    let jsonl = std::fs::read_to_string(dir.join(format!("{trace_id}.jsonl"))).unwrap();
    let lines: Vec<&str> = jsonl.lines().collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["kind"], "aise.pipeline");
    assert_eq!(first["name"], "planner");
    assert_eq!(first["payload"]["ok"], true);
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second["kind"], "aise.validation");

    let full = std::fs::read_to_string(dir.join(format!("{trace_id}.json"))).unwrap();
    let parsed: TurnTrace = serde_json::from_str(&full).unwrap();
    assert_eq!(parsed.trace_id, trace_id);
    assert_eq!(parsed.turn_id, "turn-1");
    assert_eq!(parsed.spans.len(), 2);
}
