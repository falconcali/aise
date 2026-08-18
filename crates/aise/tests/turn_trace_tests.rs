use aise::domain::ids::{StoryId, TurnNumber};
use aise::turn::turn_trace::{TraceId, TraceRecorder, TraceSpan, TraceSpanSink, TurnTrace, truncate};
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Default)]
struct RecordingSink {
    spans: Mutex<Vec<(TraceId, TraceSpan)>>,
}

impl TraceSpanSink for RecordingSink {
    fn write_span(&self, trace_id: &TraceId, span: &TraceSpan) {
        self.spans.lock().unwrap().push((trace_id.clone(), span.clone()));
    }

    fn write_trace(&self, _trace: &TurnTrace) {}
}

#[test]
fn records_nested_span_tree() {
    let mut recorder = TraceRecorder::new();
    let root = recorder.begin_span("aise.turn", "aise.turn");
    let llm = recorder.begin_span("aise.llm_call", "story_generator.llm");
    recorder.end_span_with(llm, &serde_json::json!({ "status": "ok" }));
    recorder.end_span_with(root, &serde_json::json!({ "status": "ok" }));

    let trace = recorder.build(&StoryId::try_new("story-1").unwrap(), Some(TurnNumber::try_new(1).unwrap()));
    assert_eq!(trace.trace_id, *recorder.trace_id());
    assert_eq!(trace.turn_number, Some(TurnNumber::try_new(1).unwrap()));
    assert_eq!(trace.story_id, "story-1");
    assert_eq!(trace.spans.len(), 2);
    assert_eq!(trace.spans[0].parent_span_id.as_deref(), Some(trace.spans[1].span_id.as_str()));
    assert_eq!(trace.spans[1].parent_span_id, None);
    assert!(trace.duration_ms >= trace.spans[1].duration_ms);
}

#[test]
fn record_span_attaches_to_current_parent() {
    let mut recorder = TraceRecorder::new();
    let root = recorder.begin_span("aise.turn", "aise.turn");
    recorder.record_span("aise.validation", "validation", &serde_json::json!({ "pass": true }));
    recorder.end_span_with(root, &serde_json::json!({ "status": "ok" }));

    let trace = recorder.build(&StoryId::try_new("story-1").unwrap(), Some(TurnNumber::try_new(1).unwrap()));
    assert_eq!(trace.spans.len(), 2);
    assert_eq!(trace.spans[0].parent_span_id.as_deref(), Some(trace.spans[1].span_id.as_str()));
}

#[test]
fn sink_receives_every_span_in_order() {
    let sink = Arc::new(RecordingSink::default());
    let mut recorder = TraceRecorder::new().with_sink(sink.clone());
    let root = recorder.begin_span("aise.turn", "aise.turn");
    recorder.record_span("aise.validation", "validation", &serde_json::json!({ "pass": true }));
    recorder.end_span_with(root, &serde_json::json!({ "status": "ok" }));

    let recorded = sink.spans.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    assert!(recorded.iter().all(|(id, _)| *id == *recorder.trace_id()));
    assert_eq!(recorded[0].1.kind, "aise.validation");
    assert_eq!(recorded[1].1.kind, "aise.turn");
    assert_eq!(recorded[0].1.payload["pass"], true);
}

#[test]
fn caps_span_count_and_counts_dropped() {
    let mut recorder = TraceRecorder::with_limits(2);
    for i in 0..5 {
        recorder.record_span("aise.test", &format!("span{i}"), &serde_json::json!({}));
    }
    let trace = recorder.build(&StoryId::try_new("story-1").unwrap(), Some(TurnNumber::try_new(1).unwrap()));
    assert_eq!(trace.spans.len(), 2);
    assert_eq!(trace.dropped_span_count, 3);
}

#[test]
fn new_trace_id_is_time_prefixed() {
    let trace_id = TraceRecorder::new().trace_id().clone();
    let text = trace_id.as_str().to_owned();
    assert_eq!(text.len(), 56);
    assert_eq!(trace_id.file_stem(), &text[..23]);
    assert!(text[..4].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[4..5], "-");
    assert!(text[5..7].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[7..8], "-");
    assert!(text[8..10].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[10..11], "-");
    assert!(text[11..13].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[13..14], "_");
    assert!(text[14..16].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[16..17], "_");
    assert!(text[17..19].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[19..20], "_");
    assert!(text[20..23].chars().all(|c| c.is_ascii_digit()));
    assert_eq!(&text[23..24], "-");
    assert!(text[24..].chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn truncates_long_text() {
    let long = "a".repeat(3000);
    let cut = truncate(&long, 1000);
    assert!(cut.starts_with(&"a".repeat(1000)));
    assert!(cut.ends_with("…[+2000 chars]"));
    assert_eq!(truncate("short", 1000), "short");
}
