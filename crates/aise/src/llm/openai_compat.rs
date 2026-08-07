use crate::config::{LlmConfig, LlmProtocolLimitsConfig, ThinkingMode};
use crate::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use crate::llm::error::{LlmProtocolErrorKind, LlmProviderError, LlmResponseLimit, LlmTransportErrorKind};
use crate::llm::message::{ChatMessage, CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use crate::llm::provider::{DeltaSink, LlmProvider};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

const MAX_PROVIDER_ERROR_BODY_BYTES: usize = 8 * 1024;
const MAX_PROVIDER_ERROR_CODE_CHARS: usize = 128;
const MAX_PROVIDER_ERROR_MESSAGE_CHARS: usize = 512;

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    thinking: Option<ThinkingMode>,
    protocol: LlmProtocolLimitsConfig,
}

impl OpenAiCompatProvider {
    pub fn new(config: LlmConfig) -> Self {
        let protocol = config.protocol.clone();
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            client: reqwest::Client::new(),
            thinking: config.thinking,
            protocol,
        }
    }

    fn endpoint(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    fn thinking_toggle(&self) -> Option<ThinkingToggle<'_>> {
        self.thinking.map(|mode| ThinkingToggle {
            kind: match mode {
                ThinkingMode::Enabled => "enabled",
                ThinkingMode::Disabled => "disabled",
            },
        })
    }

    fn completion_body<'a>(&'a self, req: &'a CompletionRequest, stream: bool) -> ChatCompletionRequest<'a> {
        ChatCompletionRequest {
            model: &req.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream,
            stream_options: stream.then_some(StreamOptions { include_usage: true }),
            thinking: self.thinking_toggle(),
        }
    }
}

#[derive(Serialize)]
struct ThinkingToggle<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    max_tokens: u32,
    temperature: f32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingToggle<'a>>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ResponseUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct ResponseUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: Option<u64>,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Deserialize)]
struct ProviderErrorEnvelope {
    error: ProviderErrorBody,
}

#[derive(Deserialize)]
struct ProviderErrorBody {
    code: Option<serde_json::Value>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct CompletionTokensDetails {
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn provider_name(&self) -> &'static str {
        "openai_compat"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError> {
        let body = self.completion_body(req, false);
        let mut request = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            if status.is_client_error() && status.as_u16() != 429 {
                return Err(rejected_response(response, status.as_u16()).await);
            }
            return Err(http_to_provider_error(Some(status), retry_after));
        }
        let resp: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|_| protocol_error(LlmProtocolErrorKind::InvalidJson))?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| protocol_error(LlmProtocolErrorKind::EmptyChoices))?;
        let finish_reason = choice.finish_reason.as_deref().map(parse_finish_reason).transpose()?;
        let message = choice.message;
        let usage = resp.usage.map(|u| LlmTokenUsage {
            input_tokens: u.prompt_tokens,
            cached_input_tokens: u.prompt_tokens_details.and_then(|d| d.cached_tokens),
            output_tokens: u.completion_tokens,
            reasoning_tokens: u.completion_tokens_details.and_then(|d| d.reasoning_tokens),
            total_tokens: u
                .total_tokens
                .unwrap_or_else(|| u.prompt_tokens.saturating_add(u.completion_tokens)),
            accuracy: UsageAccuracy::Exact,
        });
        Ok(LlmCompletion {
            text: message.content,
            finish_reason,
            reasoning_content: message.reasoning_content,
            usage,
            charge: None,
        })
    }

    async fn complete_stream(
        &self,
        req: &CompletionRequest,
        mut on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError> {
        let body = self.completion_body(req, true);
        let mut request = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            if status.is_client_error() && status.as_u16() != 429 {
                return Err(rejected_response(response, status.as_u16()).await);
            }
            return Err(http_to_provider_error(Some(status), retry_after));
        }
        let mut stream = response.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut finish_reason: Option<FinishReason> = None;
        let mut usage: Option<LlmTokenUsage> = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(transport_error)?;
            if buf.len().saturating_add(chunk.len()) > self.protocol.max_stream_buffer_bytes {
                return Err(LlmProviderError::ResponseLimitExceeded {
                    limit: LlmResponseLimit::StreamBuffer,
                });
            }
            buf.extend_from_slice(&chunk);
            let lines = extract_sse_lines(&mut buf, self.protocol.max_sse_line_bytes)?;
            for line in lines {
                if let Some(data) = line.strip_prefix("data: ").map(str::trim) {
                    if data == "[DONE]" {
                        return Ok(stream_completion(text, reasoning, finish_reason, usage));
                    }
                    if let Some(chunk) = parse_stream_chunk(data)? {
                        let Some(choice) = chunk.choices.first() else {
                            if let Some(chunk_usage) = chunk.usage {
                                usage = Some(response_usage_to_token_usage(chunk_usage));
                            }
                            continue;
                        };
                        if let Some(content) = &choice.delta.content {
                            if text.len().saturating_add(content.len()) > self.protocol.max_content_bytes {
                                return Err(LlmProviderError::ResponseLimitExceeded {
                                    limit: LlmResponseLimit::Content,
                                });
                            }
                            on_delta(content.clone());
                            text.push_str(content);
                        }
                        if let Some(reasoning_delta) = &choice.delta.reasoning_content {
                            if reasoning.len().saturating_add(reasoning_delta.len()) > self.protocol.max_reasoning_bytes
                            {
                                return Err(LlmProviderError::ResponseLimitExceeded {
                                    limit: LlmResponseLimit::Reasoning,
                                });
                            }
                            reasoning.push_str(reasoning_delta);
                        }
                        if let Some(reason) = &choice.finish_reason {
                            finish_reason = Some(parse_finish_reason(reason)?);
                        }
                        if let Some(chunk_usage) = chunk.usage {
                            usage = Some(response_usage_to_token_usage(chunk_usage));
                        }
                    }
                }
            }
        }
        Ok(stream_completion(text, reasoning, finish_reason, usage))
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError> {
        Err(LlmProviderError::Protocol {
            kind: LlmProtocolErrorKind::Unsupported,
        })
    }
}

fn parse_finish_reason(value: &str) -> Result<FinishReason, LlmProviderError> {
    match value {
        "stop" => Ok(FinishReason::Stop),
        "length" => Ok(FinishReason::Length),
        "content_filter" => Ok(FinishReason::ContentFilter),
        "tool_calls" => Ok(FinishReason::ToolCalls),
        _ => Err(protocol_error(LlmProtocolErrorKind::Unsupported)),
    }
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    usage: Option<ResponseUsage>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

fn parse_stream_chunk(data: &str) -> Result<Option<StreamChunk>, LlmProviderError> {
    let chunk: StreamChunk =
        serde_json::from_str(data).map_err(|_| protocol_error(LlmProtocolErrorKind::InvalidJson))?;
    if chunk.choices.is_empty() && chunk.usage.is_none() {
        return Ok(None);
    }
    Ok(Some(chunk))
}

fn stream_completion(
    content: String,
    reasoning: String,
    finish_reason: Option<FinishReason>,
    usage: Option<LlmTokenUsage>,
) -> LlmCompletion {
    let reasoning = if reasoning.trim().is_empty() {
        None
    } else {
        Some(reasoning)
    };
    LlmCompletion {
        text: content,
        finish_reason,
        reasoning_content: reasoning,
        usage,
        charge: None,
    }
}

fn response_usage_to_token_usage(usage: ResponseUsage) -> LlmTokenUsage {
    LlmTokenUsage {
        input_tokens: usage.prompt_tokens,
        cached_input_tokens: usage.prompt_tokens_details.and_then(|d| d.cached_tokens),
        output_tokens: usage.completion_tokens,
        reasoning_tokens: usage.completion_tokens_details.and_then(|d| d.reasoning_tokens),
        total_tokens: usage
            .total_tokens
            .unwrap_or_else(|| usage.prompt_tokens.saturating_add(usage.completion_tokens)),
        accuracy: UsageAccuracy::Exact,
    }
}

fn extract_sse_lines(buf: &mut Vec<u8>, max_line_bytes: usize) -> Result<Vec<String>, LlmProviderError> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut consumed = 0;
    for i in 0..buf.len() {
        if i.saturating_sub(start) > max_line_bytes {
            return Err(LlmProviderError::ResponseLimitExceeded {
                limit: LlmResponseLimit::SseLine,
            });
        }
        if buf[i] == b'\n' {
            let line = std::str::from_utf8(&buf[start..i])
                .map_err(|_| protocol_error(LlmProtocolErrorKind::InvalidSseLine))?;
            lines.push(line.to_string());
            start = i + 1;
            consumed = start;
        }
    }
    buf.drain(..consumed);
    Ok(lines)
}

fn transport_error(error: reqwest::Error) -> LlmProviderError {
    let kind = if error.is_timeout() {
        LlmTransportErrorKind::Timeout
    } else if error.is_connect() {
        LlmTransportErrorKind::Connect
    } else {
        LlmTransportErrorKind::Io
    };
    LlmProviderError::Transport { kind }
}

async fn rejected_response(response: reqwest::Response, status: u16) -> LlmProviderError {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return rejected_error(status, None, None);
        };
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_ERROR_BODY_BYTES {
            return rejected_error(status, None, None);
        }
        body.extend_from_slice(&chunk);
    }
    let Ok(envelope) = serde_json::from_slice::<ProviderErrorEnvelope>(&body) else {
        return rejected_error(status, None, None);
    };
    let code = envelope
        .error
        .code
        .and_then(provider_error_code)
        .map(|value| truncate_chars(value, MAX_PROVIDER_ERROR_CODE_CHARS));
    let message = envelope
        .error
        .message
        .map(|value| truncate_chars(value, MAX_PROVIDER_ERROR_MESSAGE_CHARS));
    rejected_error(status, code, message)
}

fn provider_error_code(value: serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn truncate_chars(value: String, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn rejected_error(status: u16, code: Option<String>, message: Option<String>) -> LlmProviderError {
    LlmProviderError::Rejected { status, code, message }
}

fn http_to_provider_error(status: Option<reqwest::StatusCode>, retry_after: Option<String>) -> LlmProviderError {
    let Some(status) = status else {
        return LlmProviderError::Transport {
            kind: LlmTransportErrorKind::Io,
        };
    };
    let code = status.as_u16();
    if code == 429 {
        let retry_after_ms = retry_after.and_then(|value| value.trim().parse::<u64>().ok().map(|secs| secs * 1_000));
        return LlmProviderError::RateLimited { retry_after_ms };
    }
    if (400..500).contains(&code) {
        return LlmProviderError::Rejected {
            status: code,
            code: None,
            message: None,
        };
    }
    if (500..600).contains(&code) {
        return LlmProviderError::Transport {
            kind: LlmTransportErrorKind::Server,
        };
    }
    LlmProviderError::Transport {
        kind: LlmTransportErrorKind::Io,
    }
}

fn protocol_error(kind: LlmProtocolErrorKind) -> LlmProviderError {
    LlmProviderError::Protocol { kind }
}

#[cfg(test)]
#[path = "tests/openai_compat_tests.rs"]
mod tests;
