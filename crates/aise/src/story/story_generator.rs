use crate::domain::ids::EventId;
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::error::AiseError;
use crate::llm::provider::LlmProvider;
use crate::runtime::pipeline::TurnExecutionPipeline;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::story::story_model::StoryDraft;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoryGenerator {
    #[allow(dead_code)]
    llm: Arc<dyn LlmProvider>,
}

impl StoryGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> &'static str {
        "story_generator"
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let story_text = "Hello World".to_string();
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
