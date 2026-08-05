use crate::core::StoryProposal;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::truncate;
use crate::error::AiseError;
use crate::llm::gateway::LlmGateway;
use crate::llm::message::CompletionSpec;
use crate::prompt::{ContextMerger, GenerationInput};
use async_trait::async_trait;
use std::sync::Arc;

const MAX_PARSE_ERROR_PREVIEW_CHARS: usize = 200;

pub struct StoryGenerator {
    gateway: Arc<LlmGateway>,
    merger: ContextMerger,
}

impl StoryGenerator {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            merger: ContextMerger,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryGenerator
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| AiseError::InvariantViolation("baseline context not set before story generation".into()))?
            .clone();
        let plan = ctx
            .plan()
            .ok_or_else(|| AiseError::InvariantViolation("writer plan not set before story generation".into()))?
            .clone();
        let retrieved = ctx.retrieved().to_vec();
        let thoughts = ctx.thoughts().to_vec();
        let issues: Vec<String> = ctx
            .validation()
            .map(|validation| validation.issues().iter().map(|issue| issue.message.clone()).collect())
            .unwrap_or_default();
        let player_input = ctx.player_input().to_string();
        let messages = self.merger.generation_messages(GenerationInput {
            baseline: &baseline,
            plan: &plan,
            retrieved: &retrieved,
            thoughts: &thoughts,
            player_input: &player_input,
            issues: &issues,
            previous_story: ctx.proposal().map(|proposal| proposal.story_text.as_str()),
        });
        let max_output = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        let spec = CompletionSpec {
            messages,
            max_output_tokens: max_output,
            purpose: "story_generation",
        };
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let completion = self.gateway.complete(scope, spec).await?;
        let proposal: StoryProposal = serde_json::from_str(&completion.text).map_err(|error| {
            AiseError::Internal(format!(
                "story proposal output is not valid JSON: {error}; raw_output={}",
                truncate(&completion.text, MAX_PARSE_ERROR_PREVIEW_CHARS)
            ))
        })?;
        ctx.set_story_proposal(proposal)
    }
}
