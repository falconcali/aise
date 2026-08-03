use crate::config::LlmConfig;
use crate::llm::error::LlmError;
use crate::llm::limiter::LlmLimiter;
use crate::llm::message::{ChatMessage, CompletionRequest};
use crate::llm::provider::{DeltaSink, LlmProvider};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    limiter: LlmLimiter,
}

impl OpenAiCompatProvider {
    pub fn new(config: LlmConfig, limiter: LlmLimiter) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            client: reqwest::Client::new(),
            limiter,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn complete(&self, req: &CompletionRequest) -> Result<String, LlmError> {
        let _permit = self.limiter.acquire().await.map_err(|e| LlmError::Protocol(e.to_string()))?;
        let body = ChatCompletionRequest {
            model: &req.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };

        let mut builder = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            builder = builder.bearer_auth(key);
        }

        let resp: ChatCompletionResponse = builder.send().await?.error_for_status()?.json().await?;
        resp.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::Protocol("empty choices".into()))
    }

    async fn complete_stream(&self, req: &CompletionRequest, mut on_delta: DeltaSink) -> Result<(), LlmError> {
        let _permit = self.limiter.acquire().await.map_err(|e| LlmError::Protocol(e.to_string()))?;
        let body = ChatCompletionRequest {
            model: &req.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
        };

        let mut builder = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            builder = builder.bearer_auth(key);
        }

        let mut stream = builder.send().await?.error_for_status()?.bytes_stream();
        let mut buf = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            for line in extract_sse_lines(&mut buf)? {
                if let Some(data) = line.strip_prefix("data: ").map(str::trim) {
                    if data == "[DONE]" {
                        return Ok(());
                    }
                    if let Some(delta) = parse_delta(data)? {
                        on_delta(delta);
                    }
                }
            }
        }
        Ok(())
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

fn parse_delta(data: &str) -> Result<Option<String>, LlmError> {
    #[derive(Deserialize)]
    struct StreamChunk {
        choices: Vec<StreamChoice>,
    }
    #[derive(Deserialize)]
    struct StreamChoice {
        delta: StreamDelta,
    }
    #[derive(Deserialize)]
    struct StreamDelta {
        content: Option<String>,
    }

    let chunk: StreamChunk = serde_json::from_str(data).map_err(|e| LlmError::Protocol(format!("bad chunk: {e}")))?;
    Ok(chunk.choices.into_iter().next().and_then(|c| c.delta.content))
}
