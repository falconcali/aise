use crate::config::LlmConfig;
use crate::domain::ids::EventId;
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::error::AiseError;
use crate::llm::message::{ChatMessage, CompletionRequest, Role};
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::story::story_model::StoryDraft;
use async_trait::async_trait;
use std::sync::Arc;
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
        let span = tracing::info_span!("llm.complete", model = %self.model, turn_id = %ctx.turn_id);
        let story_text = self
            .llm
            .complete(&CompletionRequest {
                model: self.model.clone(),
                messages: vec![ChatMessage {
                    role: Role::User,
                    content: ctx.player_input.clone(),
                }],
                max_tokens: self.max_tokens,
                temperature: self.temperature,
                stream: false,
            })
            .instrument(span)
            .await?;

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
