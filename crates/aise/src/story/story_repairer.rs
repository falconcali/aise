use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::LlmCallPurpose;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::truncate;
use crate::core::turn_validation::{ValidationDecision, ValidationIssue};
use crate::llm::gateway::LlmGateway;
use crate::llm::message::CompletionSpec;
use crate::prompt::{ContextMerger, GenerationInput};
use async_trait::async_trait;
use std::sync::Arc;

const MAX_PARSE_ERROR_PREVIEW_CHARS: usize = 200;

pub struct StoryRepairer {
    gateway: Arc<LlmGateway>,
    merger: ContextMerger,
}

impl StoryRepairer {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            merger: ContextMerger,
        }
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

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let validation = ctx
            .validation()
            .ok_or_else(|| invariant("no validation result before repair"))?;
        if validation.decision() != ValidationDecision::Repair {
            return Err(invariant("repairer only runs when validation requires repair"));
        }
        let issues: Vec<String> = validation.issues().iter().map(repair_issue_message).collect();
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before repair"))?
            .clone();
        let plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before repair"))?
            .clone();
        let retrieved = ctx.retrieved().to_vec();
        let thoughts = ctx.thoughts().to_vec();
        let previous_story = ctx.proposal().map(|proposal| proposal.story_text.as_str());
        let player_input = ctx.player_input().to_string();
        let messages = self.merger.generation_messages(GenerationInput {
            baseline: &baseline,
            plan: &plan,
            retrieved: &retrieved,
            thoughts: &thoughts,
            player_input: &player_input,
            issues: &issues,
            previous_story,
        });
        let max_output = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        let spec = CompletionSpec {
            messages,
            max_output_tokens: max_output,
            purpose: LlmCallPurpose::StoryRepair,
        };
        let mut scope = ctx.llm_call_scope(TurnStage::StoryRepairer);
        let estimated_input = crate::llm::accounting::TokenAccountant::estimate_input_tokens(&spec.messages);
        let reservation = scope.reserve_llm(estimated_input, u64::from(max_output))?;
        let completion = self.gateway.complete(scope, spec, reservation).await?;
        let proposal = serde_json::from_str(&completion.text).map_err(|error| {
            TurnExecutionError::invariant(format!(
                "story proposal output is not valid JSON: {error}; raw_output={}",
                truncate(&completion.text, MAX_PARSE_ERROR_PREVIEW_CHARS)
            ))
        })?;
        ctx.replace_story_proposal(proposal)
    }
}

fn repair_issue_message(issue: &ValidationIssue) -> String {
    format!("{}: {}", issue.code, issue.message)
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}
