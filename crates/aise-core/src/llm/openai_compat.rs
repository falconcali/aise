use crate::llm::{
    LlmCompletionFinishReason, LlmCompletionRequest, LlmCompletionResponse, LlmConfig, LlmError, LlmProvider,
};
use crate::core::{ChatMessage, ChatMessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    http_client: reqwest::Client,
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiChatMessage<'a>>,
    temperature: f32,
}

#[derive(Serialize)]
struct OpenAiChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChatChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiChatChoiceMessage {
    content: Option<String>,
}

impl OpenAiCompatProvider {
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            base_url: config.base_url.clone(),
            api_key: config.api_key.clone(),
            http_client: reqwest::Client::new(),
        }
    }

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    async fn send(&self, request: &LlmCompletionRequest) -> Result<String, LlmError> {
        let response = self
            .http_client
            .post(self.completions_url())
            .bearer_auth(&self.api_key)
            .json(&to_chat_request(request))
            .send()
            .await
            .map_err(map_transport_error)?;
        let status = response.status();
        let body = response.text().await.map_err(map_transport_error)?;
        if !status.is_success() {
            return Err(LlmError::Rejected {
                status: status.as_u16(),
                message: (!body.is_empty()).then_some(body),
            });
        }
        Ok(body)
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &'static str {
        "openai_compat"
    }

    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse, LlmError> {
        let body = self.send(&request).await?;
        parse_chat_response(&body)
    }
}

fn to_chat_request(request: &LlmCompletionRequest) -> OpenAiChatRequest<'_> {
    OpenAiChatRequest {
        model: &request.model,
        messages: request.messages.iter().map(to_chat_message).collect(),
        temperature: request.temperature,
    }
}

fn to_chat_message(message: &ChatMessage) -> OpenAiChatMessage<'_> {
    let role = match message.role {
        ChatMessageRole::System => "system",
        ChatMessageRole::User => "user",
    };
    OpenAiChatMessage {
        role,
        content: &message.content,
    }
}

fn parse_chat_response(body: &str) -> Result<LlmCompletionResponse, LlmError> {
    let response: OpenAiChatResponse = serde_json::from_str(body).map_err(|_| LlmError::InvalidResponse {
        reason: "malformed chat completion json",
    })?;
    let choice = response.choices.into_iter().next().ok_or(LlmError::InvalidResponse {
        reason: "missing choices",
    })?;
    let content = choice
        .message
        .content
        .filter(|content| !content.is_empty())
        .ok_or(LlmError::EmptyCompletion)?;
    Ok(LlmCompletionResponse {
        content,
        finish_reason: map_finish_reason(choice.finish_reason.as_deref()),
    })
}

fn map_finish_reason(reason: Option<&str>) -> LlmCompletionFinishReason {
    match reason {
        Some("stop") => LlmCompletionFinishReason::Stop,
        Some("length") => LlmCompletionFinishReason::Length,
        Some("content_filter") => LlmCompletionFinishReason::ContentFilter,
        _ => LlmCompletionFinishReason::Other,
    }
}

fn map_transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Transport {
            message: error.to_string(),
        }
    }
}
