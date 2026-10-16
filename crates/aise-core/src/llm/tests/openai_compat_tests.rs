use super::*;

fn request(reasoning_effort: Option<LlmReasoningEffort>) -> LlmCompletionRequest {
    LlmCompletionRequest {
        model: "model".to_owned(),
        messages: vec![ChatMessage::user("input")],
        temperature: 0.7,
        reasoning_effort,
    }
}

#[test]
fn default_reasoning_effort_serializes_as_low() {
    let reasoning_effort = LlmReasoningEffort::default();
    let value =
        serde_json::to_value(to_chat_request(&request(Some(reasoning_effort)))).expect("serialize chat request");

    assert_eq!(reasoning_effort, LlmReasoningEffort::Low);
    assert_eq!(value["reasoning_effort"], "low");
}

#[test]
fn serializes_disabled_reasoning_effort() {
    let value = serde_json::to_value(to_chat_request(&request(Some(LlmReasoningEffort::None))))
        .expect("serialize chat request");

    assert_eq!(value["reasoning_effort"], "none");
}

#[test]
fn omits_unspecified_reasoning_effort() {
    let value = serde_json::to_value(to_chat_request(&request(None))).expect("serialize chat request");

    assert!(value.get("reasoning_effort").is_none());
}

#[test]
fn parses_token_usage_details() {
    let body = r#"{
        "choices": [{
            "message": {"content": "done"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 17903,
            "completion_tokens": 188,
            "total_tokens": 18091,
            "prompt_tokens_details": {"cached_tokens": 17817},
            "completion_tokens_details": {"reasoning_tokens": 120}
        }
    }"#;

    let response = parse_chat_response(body).expect("valid chat completion");

    assert_eq!(
        response.usage,
        Some(LlmTokenUsage {
            input_tokens: 17903,
            cached_input_tokens: Some(17817),
            output_tokens: 188,
            reasoning_tokens: Some(120),
            total_tokens: 18091,
        })
    );
}

#[test]
fn accepts_response_without_token_usage() {
    let body = r#"{
        "choices": [{
            "message": {"content": "done"},
            "finish_reason": "stop"
        }]
    }"#;

    let response = parse_chat_response(body).expect("valid chat completion");

    assert_eq!(response.usage, None);
}

#[test]
fn derives_missing_total_tokens() {
    let body = r#"{
        "choices": [{
            "message": {"content": "done"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5
        }
    }"#;

    let response = parse_chat_response(body).expect("valid chat completion");

    assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(15));
}
