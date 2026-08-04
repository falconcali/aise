use crate::config::LlmConfig;
use crate::core::story_proposal::StoryProposal;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::{
    LlmCallData, MAX_LLM_CONTENT_CHARS, MAX_LLM_RESPONSE_CHARS, MessageData, SpanPayload, truncate,
};
use crate::domain::ids::EventId;
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::error::AiseError;
use crate::llm::message::{ChatMessage, CompletionRequest, Role};
use crate::llm::provider::LlmProvider;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::Instrument;

pub struct StoryGenerator {
    llm: Arc<dyn LlmProvider>,
    model: String,
    temperature: f32,
}

impl StoryGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>, config: &LlmConfig) -> Self {
        Self {
            llm,
            model: config.model.clone(),
            temperature: config.temperature,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryGenerator
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        ctx.complete_context_preparation()?;
        let request = CompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: ctx.player_input().to_string(),
            }],
            max_tokens: ctx.budget().max_tokens(),
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

        let span = tracing::info_span!("llm.complete", model = %self.model, turn_id = %ctx.turn_id());
        let pending = ctx.trace().begin_span("aise.llm_call", "story_generator.llm");
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
        ctx.trace().end_span_with(pending, &payload);

        let story_text = outcome?;
        let turn_id = ctx.turn_id().clone();
        let event = StoryEvent {
            id: EventId::from(format!("{turn_id}#0")),
            turn_id,
            seq: 0,
            kind: EventKind::Action,
            payload: serde_json::json!({ "text": story_text }),
        };
        ctx.set_story_proposal(StoryProposal {
            story_text,
            events: vec![event],
            character_updates: Vec::new(),
            world_updates: Vec::new(),
            memory_updates: Vec::new(),
        })
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
