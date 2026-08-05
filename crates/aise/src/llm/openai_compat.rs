use crate::config::{LlmConfig, ThinkingMode};
use crate::llm::accounting::{FinishReason, LlmCompletion, LlmTokenUsage, UsageAccuracy};
use crate::llm::error::LlmError;
use crate::llm::message::{ChatMessage, CompletionRequest, EmbeddingOutput, EmbeddingRequest};
use crate::llm::provider::{DeltaSink, LlmProvider};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    thinking: Option<ThinkingMode>,
}

impl OpenAiCompatProvider {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            client: reqwest::Client::new(),
            thinking: config.thinking,
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
    thinking: Option<ThinkingToggle<'a>>,
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
    completion_tokens_details: Option<CompletionTokensDetails>,
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

    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmError> {
        let body = ChatCompletionRequest {
            model: &req.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
            thinking: self.thinking_toggle(),
        };
        let mut request = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let resp: ChatCompletionResponse = request.send().await?.error_for_status()?.json().await?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Protocol("empty choices".into()))?;
        let finish_reason = choice.finish_reason.as_deref().map(parse_finish_reason);
        let message = choice.message;
        let usage = resp.usage.map(|u| LlmTokenUsage {
            input_tokens: u.prompt_tokens,
            cached_input_tokens: None,
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
    ) -> Result<LlmCompletion, LlmError> {
        let body = ChatCompletionRequest {
            model: &req.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
            thinking: self.thinking_toggle(),
        };
        let mut request = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let mut stream = request.send().await?.error_for_status()?.bytes_stream();
        let mut buf = Vec::new();
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            for line in extract_sse_lines(&mut buf)? {
                if let Some(data) = line.strip_prefix("data: ").map(str::trim) {
                    if data == "[DONE]" {
                        return Ok(stream_completion(text, reasoning));
                    }
                    if let Some(delta) = parse_delta(data)? {
                        if let Some(content) = delta.content {
                            on_delta(content.clone());
                            text.push_str(&content);
                        }
                        if let Some(reasoning_delta) = delta.reasoning_content {
                            reasoning.push_str(&reasoning_delta);
                        }
                    }
                }
            }
        }
        Ok(stream_completion(text, reasoning))
    }

    async fn embed(&self, _req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmError> {
        Err(LlmError::EmbeddingUnsupported)
    }
}

fn parse_finish_reason(value: &str) -> FinishReason {
    match value {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "tool_calls" => FinishReason::ToolCalls,
        other => FinishReason::Other(other.to_owned()),
    }
}

fn extract_sse_lines(buf: &mut Vec<u8>) -> Result<Vec<String>, LlmError> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut consumed = 0;
    for i in 0..buf.len() {
        if buf[i] == b'\n' {
            let line = std::str::from_utf8(&buf[start..i])
                .map_err(|e| LlmError::Protocol(format!("invalid utf-8 in SSE stream: {e}")))?;
            lines.push(line.to_string());
            start = i + 1;
            consumed = start;
        }
    }
    buf.drain(..consumed);
    Ok(lines)
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

fn parse_delta(data: &str) -> Result<Option<StreamDelta>, LlmError> {
    #[derive(Deserialize)]
    struct StreamChunk {
        choices: Vec<StreamChoice>,
    }
    #[derive(Deserialize)]
    struct StreamChoice {
        delta: StreamDelta,
    }

    let chunk: StreamChunk = serde_json::from_str(data).map_err(|e| LlmError::Protocol(format!("bad chunk: {e}")))?;
    Ok(chunk.choices.into_iter().next().map(|c| c.delta))
}

fn stream_completion(content: String, reasoning: String) -> LlmCompletion {
    let reasoning = if reasoning.trim().is_empty() {
        None
    } else {
        Some(reasoning)
    };
    LlmCompletion {
        text: content,
        finish_reason: Some(FinishReason::Stop),
        reasoning_content: reasoning,
        usage: None,
        charge: None,
    }
}

#[cfg(test)]
#[path = "tests/openai_compat_tests.rs"]
mod tests;
