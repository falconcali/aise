use crate::core::story_proposal::{ProposedEvent, ProposedWorldChange, StoryProposal};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::domain::narrative::EventKind;
use crate::error::AiseError;
use crate::llm::gateway::LlmGateway;
use crate::llm::message::{ChatMessage, CompletionSpec, Role};
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoryRepairer {
    gateway: Arc<LlmGateway>,
}

impl StoryRepairer {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }

    pub fn gateway(&self) -> &Arc<LlmGateway> {
        &self.gateway
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryRepairer {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryRepairer
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let player_input = ctx.player_input().to_string();
        let issues_detail = {
            let issues = ctx.validation().map(|result| result.issues()).unwrap_or_default();
            issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let instruction = if issues_detail.is_empty() {
            format!("player input: {player_input}")
        } else {
            format!("player input: {player_input}; fix these validation issues: {issues_detail}")
        };
        let max_output = ctx.budget().remaining_output_tokens().min(u32::MAX as u64) as u32;
        let spec = CompletionSpec {
            messages: vec![ChatMessage {
                role: Role::User,
                content: instruction,
            }],
            max_output_tokens: max_output,
            purpose: "story_repair",
        };
        let scope = ctx.llm_call_scope(TurnStage::StoryRepairer);
        let completion = self.gateway.complete(scope, spec).await?;
        let text = completion.text;
        let summary_delta = ctx.proposal().and_then(|proposal| proposal.summary_delta.clone());
        ctx.replace_story_proposal(StoryProposal {
            story_text: text.clone(),
            events: vec![ProposedEvent {
                kind: EventKind::Action,
                summary: text,
            }],
            character_changes: Vec::new(),
            world_change: ProposedWorldChange::default(),
            memory_changes: Vec::new(),
            summary_delta,
        })
    }
}
