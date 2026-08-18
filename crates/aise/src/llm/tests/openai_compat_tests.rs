use super::*;
use crate::config::StructuredOutputMode;
use crate::llm::accounting::FinishReason;
use crate::llm::error::{LlmProviderError, LlmResponseLimit, LlmTransportErrorKind};
use crate::llm::message::{ChatMessage, CompletionRequest, Role};
use crate::llm::output_contract::{
    CompletionOutputRequest, ProviderTransportCapabilities, ResolvedStructuredOutputRequest,
};
use crate::llm::provider::DeltaSink;
use crate::turn::turn_contract::LlmCallPurpose;
use std::sync::{Arc, Mutex};

fn provider_with(base_url: &str) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(LlmConfig {
        base_url: base_url.into(),
        api_key: None,
        model: "test".into(),
        max_concurrent: 1,
        temperature: 0.0,
        ..LlmConfig::default()
    })
}

fn provider_with_limits(base_url: &str, protocol: LlmProtocolLimitsConfig) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(LlmConfig {
        base_url: base_url.into(),
        api_key: None,
        model: "test".into(),
        max_concurrent: 1,
        temperature: 0.0,
        protocol,
        ..LlmConfig::default()
    })
}

fn request() -> CompletionRequest {
    CompletionRequest {
        model: "test".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "hello".into(),
        }],
        max_tokens: 64,
        temperature: 0.0,
        purpose: LlmCallPurpose::StoryGeneration,
        output: CompletionOutputRequest::Text,
    }
}

fn structured_request(mode: StructuredOutputMode) -> CompletionRequest {
    let schema = Arc::new(serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["decision"],
        "properties": {"decision": {"type": "string"}}
    }));
    CompletionRequest {
        output: CompletionOutputRequest::Structured(ResolvedStructuredOutputRequest {
            contract_name: "test_contract",
            schema_hash: crate::domain::asset::ids::Sha256Digest::from_bytes([0u8; 32]),
            schema,
            mode,
        }),
        ..request()
    }
}

async fn serve(status: u16, headers: &[(&str, &str)], body: &str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let body = body.to_string();
    let headers: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept test connection");
        let mut buf = vec![0u8; 8192];
        let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
        let mut head = format!("HTTP/1.1 {status} {}\r\n", if status == 200 { "OK" } else { "Error" });
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        for (key, value) in headers {
            head.push_str(&format!("{key}: {value}\r\n"));
        }
        let mut response = head.into_bytes();
        response.extend_from_slice(b"\r\n");
        response.extend_from_slice(body.as_bytes());
        let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, &response).await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn stream_parses_finish_reason_and_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"length\"}],\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":12,\"total_tokens\":42,\"prompt_tokens_details\":{\"cached_tokens\":7},\"completion_tokens_details\":{\"reasoning_tokens\":5}}}\n\n",
        "data: [DONE]\n\n",
    );
    let base_url = serve(200, &[], body).await;
    let provider = provider_with(&base_url);

    let deltas: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collected = deltas.clone();
    let sink: DeltaSink = Box::new(move |delta| collected.lock().expect("lock deltas").push(delta));
    let completion = provider.complete_stream(&request(), sink).await.expect("stream completes");
    assert_eq!(completion.text, "Hello");
    assert_eq!(completion.finish_reason, Some(FinishReason::Length));
    let usage = completion.usage.expect("stream usage parsed");
    assert_eq!(usage.input_tokens, 30);
    assert_eq!(usage.cached_input_tokens, Some(7));
    assert_eq!(usage.output_tokens, 12);
    assert_eq!(usage.reasoning_tokens, Some(5));
    let deltas = deltas.lock().expect("lock deltas");
    assert_eq!(*deltas, vec!["Hel".to_string(), "lo".to_string()]);
}

#[tokio::test]
async fn stream_rejects_line_buffer_content_and_reasoning_overflow() {
    let default_protocol = LlmProtocolLimitsConfig::default();

    let line_url = serve(200, &[], "data: \"a very long line that exceeds the sse line limit\"\n\n").await;
    let provider = provider_with_limits(
        &line_url,
        LlmProtocolLimitsConfig {
            max_sse_line_bytes: 16,
            ..default_protocol.clone()
        },
    );
    let error = provider.complete_stream(&request(), Box::new(|_| {})).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::ResponseLimitExceeded {
                limit: LlmResponseLimit::SseLine
            }
        ),
        "unexpected error: {error}"
    );

    let buffer_url = serve(200, &[], &"x".repeat(4 * 1024)).await;
    let provider = provider_with_limits(
        &buffer_url,
        LlmProtocolLimitsConfig {
            max_stream_buffer_bytes: 256,
            ..default_protocol.clone()
        },
    );
    let error = provider.complete_stream(&request(), Box::new(|_| {})).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::ResponseLimitExceeded {
                limit: LlmResponseLimit::StreamBuffer
            }
        ),
        "unexpected error: {error}"
    );

    let content_url = serve(
        200,
        &[],
        "data: {\"choices\":[{\"delta\":{\"content\":\"abcdefghij\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n",
    )
    .await;
    let provider = provider_with_limits(
        &content_url,
        LlmProtocolLimitsConfig {
            max_content_bytes: 8,
            ..default_protocol.clone()
        },
    );
    let error = provider.complete_stream(&request(), Box::new(|_| {})).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::ResponseLimitExceeded {
                limit: LlmResponseLimit::Content
            }
        ),
        "unexpected error: {error}"
    );

    let reasoning_url = serve(
        200,
        &[],
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"abcdefghij\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n",
    )
    .await;
    let provider = provider_with_limits(
        &reasoning_url,
        LlmProtocolLimitsConfig {
            max_reasoning_bytes: 8,
            ..default_protocol.clone()
        },
    );
    let error = provider.complete_stream(&request(), Box::new(|_| {})).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::ResponseLimitExceeded {
                limit: LlmResponseLimit::Reasoning
            }
        ),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn provider_classifies_429_4xx_and_5xx() {
    let rate_limit_url = serve(429, &[("Retry-After", "2")], "").await;
    let provider = provider_with(&rate_limit_url);
    let error = provider.complete(&request()).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::RateLimited {
                retry_after_ms: Some(2000)
            }
        ),
        "429 must parse Retry-After seconds: {error}"
    );

    let rejected_url = serve(
        400,
        &[],
        r#"{"error":{"message":"stream_options should be set along with stream = true","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#,
    )
    .await;
    let provider = provider_with(&rejected_url);
    let error = provider.complete(&request()).await.unwrap_err();
    match error {
        LlmProviderError::Rejected { status, code, message } => {
            assert_eq!(status, 400);
            assert_eq!(code.as_deref(), Some("invalid_request_error"));
            assert_eq!(
                message.as_deref(),
                Some("stream_options should be set along with stream = true")
            );
        }
        other => panic!("4xx maps to Rejected: {other}"),
    }

    let server_url = serve(500, &[], "").await;
    let provider = provider_with(&server_url);
    let error = provider.complete(&request()).await.unwrap_err();
    assert!(
        matches!(
            error,
            LlmProviderError::Transport {
                kind: LlmTransportErrorKind::Server
            }
        ),
        "5xx maps to Server transport: {error}"
    );
}

#[test]
fn endpoint_appends_chat_completions_to_plain_base() {
    let provider = provider_with("https://api.deepseek.com");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/chat/completions");
}

#[test]
fn endpoint_appends_to_v1_base() {
    let provider = provider_with("https://api.deepseek.com/v1");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/v1/chat/completions");
}

#[test]
fn endpoint_keeps_existing_chat_completions_path() {
    let provider = provider_with("https://api.deepseek.com/chat/completions");
    assert_eq!(provider.endpoint(), "https://api.deepseek.com/chat/completions");
}

#[test]
fn thinking_toggle_serializes_when_configured() {
    let provider = OpenAiCompatProvider::new(LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        thinking: Some(ThinkingMode::Disabled),
        ..LlmConfig::default()
    });
    let toggle = provider.thinking_toggle().expect("toggle configured");
    let value = serde_json::to_value(toggle).expect("serialize toggle");
    assert_eq!(value, serde_json::json!({"type": "disabled"}));
}

#[test]
fn thinking_toggle_omitted_when_not_configured() {
    let provider = provider_with("https://api.deepseek.com");
    assert!(provider.thinking_toggle().is_none());
}

#[test]
fn stream_options_are_serialized_only_for_streaming_requests() {
    let provider = provider_with("https://api.deepseek.com");
    let request = request();
    let non_streaming = serde_json::to_value(provider.completion_body(&request, false)).expect("serialize request");
    assert_eq!(non_streaming.get("stream"), Some(&serde_json::json!(false)));
    assert!(non_streaming.get("stream_options").is_none());

    let streaming = serde_json::to_value(provider.completion_body(&request, true)).expect("serialize request");
    assert_eq!(streaming.get("stream"), Some(&serde_json::json!(true)));
    assert_eq!(
        streaming.get("stream_options"),
        Some(&serde_json::json!({"include_usage": true}))
    );
}

#[tokio::test]
async fn oversized_provider_error_body_is_not_retained() {
    let body = format!(
        "{{\"error\":{{\"message\":\"{}\",\"code\":\"too_large\"}}}}",
        "x".repeat(MAX_PROVIDER_ERROR_BODY_BYTES)
    );
    let rejected_url = serve(400, &[], &body).await;
    let provider = provider_with(&rejected_url);
    let error = provider.complete(&request()).await.unwrap_err();
    assert!(matches!(
        error,
        LlmProviderError::Rejected {
            status: 400,
            code: None,
            message: None,
        }
    ));
}

#[test]
fn transport_capabilities_report_all_four_modes() {
    let provider = provider_with("https://api.deepseek.com");
    let capabilities: ProviderTransportCapabilities = provider.transport_capabilities();
    for mode in [
        StructuredOutputMode::NativeJsonSchema,
        StructuredOutputMode::ForcedStrictTool,
        StructuredOutputMode::JsonObject,
        StructuredOutputMode::PromptFallback,
    ] {
        assert!(capabilities.encodable_modes.contains(&mode), "missing mode {mode:?}");
    }
}

#[test]
fn native_json_schema_mode_sets_response_format() {
    let provider = provider_with("https://api.deepseek.com");
    let request = structured_request(StructuredOutputMode::NativeJsonSchema);
    let body = serde_json::to_value(provider.completion_body(&request, false)).expect("serialize");
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["name"], "test_contract");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert!(body.get("tools").is_none());
}

#[test]
fn forced_strict_tool_mode_sets_tools_and_tool_choice() {
    let provider = provider_with("https://api.deepseek.com");
    let request = structured_request(StructuredOutputMode::ForcedStrictTool);
    let body = serde_json::to_value(provider.completion_body(&request, false)).expect("serialize");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "test_contract");
    assert_eq!(body["tool_choice"]["function"]["name"], "test_contract");
    assert!(body.get("response_format").is_none());
}

#[test]
fn json_object_mode_sets_response_format_without_schema() {
    let provider = provider_with("https://api.deepseek.com");
    let request = structured_request(StructuredOutputMode::JsonObject);
    let body = serde_json::to_value(provider.completion_body(&request, false)).expect("serialize");
    assert_eq!(body["response_format"]["type"], "json_object");
    assert!(body["response_format"].get("json_schema").is_none());
}

#[test]
fn prompt_fallback_mode_adds_no_transport_field() {
    let provider = provider_with("https://api.deepseek.com");
    let request = structured_request(StructuredOutputMode::PromptFallback);
    let body = serde_json::to_value(provider.completion_body(&request, false)).expect("serialize");
    assert!(body.get("response_format").is_none());
    assert!(body.get("tools").is_none());
}

#[tokio::test]
async fn forced_strict_tool_response_extracts_the_single_matching_call() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "content": "",
                "tool_calls": [{"function": {"name": "test_contract", "arguments": "{\"decision\":\"wait\"}"}}]
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string();
    let base_url = serve(200, &[], &body).await;
    let provider = provider_with(&base_url);
    let completion = provider
        .complete(&structured_request(StructuredOutputMode::ForcedStrictTool))
        .await
        .expect("tool call extracted");
    assert_eq!(completion.text, "{\"decision\":\"wait\"}");
}

#[tokio::test]
async fn forced_strict_tool_response_rejects_competing_prose() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "content": "not empty",
                "tool_calls": [{"function": {"name": "test_contract", "arguments": "{}"}}]
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string();
    let base_url = serve(200, &[], &body).await;
    let provider = provider_with(&base_url);
    let error = provider
        .complete(&structured_request(StructuredOutputMode::ForcedStrictTool))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LlmProviderError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput
        }
    ));
}

#[tokio::test]
async fn forced_strict_tool_response_rejects_zero_calls() {
    let body = serde_json::json!({
        "choices": [{
            "message": {"content": ""},
            "finish_reason": "stop"
        }]
    })
    .to_string();
    let base_url = serve(200, &[], &body).await;
    let provider = provider_with(&base_url);
    let error = provider
        .complete(&structured_request(StructuredOutputMode::ForcedStrictTool))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LlmProviderError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput
        }
    ));
}

#[tokio::test]
async fn forced_strict_tool_response_rejects_wrong_function_name() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "content": "",
                "tool_calls": [{"function": {"name": "other_contract", "arguments": "{}"}}]
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string();
    let base_url = serve(200, &[], &body).await;
    let provider = provider_with(&base_url);
    let error = provider
        .complete(&structured_request(StructuredOutputMode::ForcedStrictTool))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LlmProviderError::Protocol {
            kind: crate::llm::error::LlmProtocolErrorKind::InvalidStructuredOutput
        }
    ));
}
