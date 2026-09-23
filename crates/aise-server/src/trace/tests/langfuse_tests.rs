use super::*;
use aise::turn::turn_trace::{LlmCallContent, MessageData};

fn llm_data() -> LlmCallData {
    LlmCallData {
        provider: "openai-compatible".into(),
        model: "model-1".into(),
        purpose: "story_generation".into(),
        stream: false,
        attempt: 1,
        queue_wait_ms: 2,
        provider_latency_ms: 20,
        total_latency_ms: 22,
        input_tokens: 100,
        cached_input_tokens: Some(40),
        output_tokens: 30,
        reasoning_tokens: Some(10),
        usage_accuracy: "provider_reported".into(),
        finish_reason: Some("stop".into()),
        charge: None,
        status: "succeeded".into(),
        error_kind: None,
        content: Some(LlmCallContent {
            messages: vec![MessageData {
                role: "user".into(),
                content: "hello".into(),
            }],
            response: "world".into(),
        }),
        structured_output: None,
    }
}

fn trace_span(payload: SpanPayload) -> TraceSpan {
    TraceSpan {
        span_id: "span-1".into(),
        parent_span_id: None,
        kind: "aise.llm_call".into(),
        name: "llm.call".into(),
        started_at_ms: 1,
        ended_at_ms: 2,
        duration_ms: 1,
        payload: serde_json::to_value(payload).unwrap(),
    }
}

#[test]
fn generation_has_stable_name_and_type() {
    let span = trace_span(SpanPayload::LlmCall(Box::new(llm_data())));
    let payload = parse_payload(&span);
    assert_eq!(observation_name(&span, payload.as_ref()), "generate-story");
    assert_eq!(observation_type(&span, payload.as_ref()), "generation");
}

#[test]
fn usage_buckets_do_not_overlap() {
    let usage: serde_json::Value = serde_json::from_str(&usage_json(&llm_data())).unwrap();
    assert_eq!(usage["input"], 60);
    assert_eq!(usage["input_cached_tokens"], 40);
    assert_eq!(usage["output"], 20);
    assert_eq!(usage["output_reasoning_tokens"], 10);
    assert_eq!(usage["total"], 130);
}

#[test]
fn trace_output_uses_latest_successful_story_response() {
    let first = trace_span(SpanPayload::LlmCall(Box::new(llm_data())));
    let mut repaired = llm_data();
    repaired.purpose = "story_repair".into();
    repaired.content.as_mut().unwrap().response = "repaired".into();
    let mut second = trace_span(SpanPayload::LlmCall(Box::new(repaired)));
    second.span_id = "span-2".into();
    let trace = TurnTrace {
        trace_id: TraceId::try_new("trace-1").unwrap(),
        story_id: "story-1".into(),
        turn_number: None,
        started_at_ms: 1,
        ended_at_ms: 2,
        duration_ms: 1,
        dropped_span_count: 0,
        spans: vec![first, second],
    };
    assert_eq!(trace_output(&trace).as_deref(), Some("repaired"));
}

#[test]
fn otlp_payload_preserves_hierarchy_and_content_policy() {
    let root = TraceSpan {
        span_id: "11111111111111112222222222222222".into(),
        parent_span_id: None,
        kind: "aise.turn".into(),
        name: "aise.turn".into(),
        started_at_ms: 1,
        ended_at_ms: 4,
        duration_ms: 3,
        payload: serde_json::to_value(SpanPayload::Turn(TurnData {
            story_id: "story-1".into(),
            turn_number: None,
            player_contribution: "secret input".into(),
            status: "ok".into(),
            error: None,
        }))
        .unwrap(),
    };
    let mut generation = trace_span(SpanPayload::LlmCall(Box::new(llm_data())));
    generation.parent_span_id = Some(root.span_id.clone());
    let trace = TurnTrace {
        trace_id: TraceId::try_new("2026-09-21-00_00_00_000-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        story_id: "story-1".into(),
        turn_number: None,
        started_at_ms: 1,
        ended_at_ms: 4,
        duration_ms: 3,
        dropped_span_count: 0,
        spans: vec![generation, root],
    };
    let hidden = otlp_payload(std::slice::from_ref(&trace), false, "test");
    let hidden_spans = hidden["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().unwrap();
    assert_eq!(hidden_spans.len(), 2);
    assert_eq!(hidden_spans[1]["parentSpanId"], hidden_spans[0]["spanId"]);
    assert!(
        hidden_spans[0]["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|attribute| attribute["key"] != "langfuse.observation.input")
    );
    let captured = otlp_payload(std::slice::from_ref(&trace), true, "test");
    let captured_root = &captured["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
    assert!(
        captured_root["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|attribute| attribute["key"] == "langfuse.observation.input")
    );
}
