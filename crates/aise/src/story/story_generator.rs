use crate::core::story_proposal::{ProposedEvent, ProposedWorldChange, StoryProposal};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::domain::narrative::EventKind;
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
        ctx.set_story_proposal(StoryProposal {
            story_text: story_text.clone(),
            events: vec![ProposedEvent {
                kind: EventKind::Action,
                summary: story_text,
            }],
            character_changes: Vec::new(),
            world_change: ProposedWorldChange::default(),
            memory_changes: Vec::new(),
            summary_delta: None,
        })
    }
}
