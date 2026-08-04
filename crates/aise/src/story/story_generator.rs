use crate::core::story_proposal::StoryProposal;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::domain::ids::EventId;
use crate::domain::narrative::{EventKind, StoryEvent};
use crate::error::AiseError;
use crate::llm::gateway::LlmGateway;
use crate::llm::message::{ChatMessage, CompletionSpec, Role};
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoryGenerator {
    gateway: Arc<LlmGateway>,
}

impl StoryGenerator {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryGenerator
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        ctx.complete_context_preparation()?;
        let max_output = ctx.budget().remaining_output_tokens().min(u32::MAX as u64) as u32;
        let spec = CompletionSpec {
            messages: vec![ChatMessage {
                role: Role::User,
                content: ctx.player_input().to_string(),
            }],
            max_output_tokens: max_output,
            purpose: "story_generate",
        };
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let completion = self.gateway.complete(scope, spec).await?;
        let story_text = completion.text;
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
