use crate::config::LlmConfig;
use crate::domain::ids::EventId;
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::error::AiseError;
use crate::llm::message::{ChatMessage, CompletionRequest, Role};
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::trace::{
    LlmCallData, MAX_LLM_CONTENT_CHARS, MAX_LLM_RESPONSE_CHARS, MessageData, SpanPayload, truncate,
};
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::story::story_model::StoryDraft;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::Instrument;
use uuid::Uuid;

pub struct StoryGenerator {
    llm: Arc<dyn LlmProvider>,
    model: String,
    temperature: f32,
    max_tokens: u32,
}

impl StoryGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>, config: &LlmConfig, max_tokens: u32) -> Self {
        Self {
            llm,
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> &'static str {
        "story_generator"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let request = CompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: ctx.player_input.clone(),
            }],
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            stream: false,
        };
        let messages: Vec<MessageData> = request
            .messages
            .iter()
            .map(|message| MessageData {
                role: role_label(message.role).to_owned(),
                content: truncate(&message.content, MAX_LLM_CONTENT_CHARS),
            })
            .collect();

        let span = tracing::info_span!("llm.complete", model = %self.model, turn_id = %ctx.turn_id);
        let pending = ctx.trace.begin_span("aise.llm_call", "story_generator.llm");
        let started = Instant::now();
        let outcome = self.llm.complete(&request).instrument(span).await;
        let latency_ms = started.elapsed().as_millis() as u64;

        let payload = match &outcome {
            Ok(text) => SpanPayload::LlmCall(LlmCallData {
                model: request.model.clone(),
                messages,
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                stream: request.stream,
                status: "ok".into(),
                response: Some(truncate(text, MAX_LLM_RESPONSE_CHARS)),
                error: None,
                latency_ms,
            }),
            Err(error) => SpanPayload::LlmCall(LlmCallData {
                model: request.model.clone(),
                messages,
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                stream: request.stream,
                status: "error".into(),
                response: None,
                error: Some(error.to_string()),
                latency_ms,
            }),
        };
        ctx.trace.end_span_with(pending, &payload);

        let story_text = outcome?;
        let event = StoryEvent {
            id: EventId::from(Uuid::new_v4().to_string()),
            turn_id: ctx.turn_id.clone(),
            seq: 0,
            kind: EventKind::Action,
            payload: serde_json::json!({ "text": story_text }),
        };
        ctx.draft = Some(StoryDraft {
            story_text,
            events: vec![event],
            character_updates: Vec::new(),
            world_updates: Vec::new(),
            memory_updates: Vec::new(),
        });
        Ok(())
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
