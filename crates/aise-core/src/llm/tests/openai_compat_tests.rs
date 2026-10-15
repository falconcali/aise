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
fn default_reasoning_effort_serializes_as_medium() {
    let reasoning_effort = LlmReasoningEffort::default();
    let value =
        serde_json::to_value(to_chat_request(&request(Some(reasoning_effort)))).expect("serialize chat request");

    assert_eq!(reasoning_effort, LlmReasoningEffort::Medium);
    assert_eq!(value["reasoning_effort"], "medium");
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
