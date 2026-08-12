use crate::domain::turn::StoryProposal;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{ModelRequest, PromptProfile};
use crate::story::story_generator_prompt::{
    DefaultStoryGeneratorPromptContextProjector, StoryGeneratorPromptContext, StoryGeneratorPromptContextProjector,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::LlmCallPurpose;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_validation::{ValidationDecision, ValidationIssue};
use async_trait::async_trait;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
struct StoryRepairerContext {
    generation: StoryGeneratorPromptContext,
    previous_proposal: StoryProposal,
    issues: Vec<ValidationIssue>,
}

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

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let validation = ctx
            .validation()
            .ok_or_else(|| invariant("no validation result before repair"))?;
        if validation.decision() != ValidationDecision::Repair {
            return Err(invariant("repairer only runs when validation requires repair"));
        }
        let issues = validation.issues().to_vec();
        let previous_proposal = ctx
            .proposal()
            .ok_or_else(|| invariant("proposal not set before repair"))?
            .clone();
        let generation = DefaultStoryGeneratorPromptContextProjector
            .project(ctx)
            .map_err(|error| invariant(error.to_string()))?
            .context;
        let request = ModelRequest::new(
            PromptProfile::StoryRepairer,
            StoryRepairerContext {
                generation,
                previous_proposal,
                issues,
            },
            ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32,
            LlmCallPurpose::StoryRepair,
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryRepairer);
        let completion = self.gateway.complete_typed(scope, request).await.map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "llm_error",
                Some(TurnStage::StoryRepairer),
                error.to_string(),
            )
        })?;
        let proposal: StoryProposal = serde_json::from_str(&completion.text).map_err(|_| {
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryRepairer),
                "story repair output is invalid",
            )
        })?;
        if !proposal.is_within_bounds(
            ctx.budget().max_total_items(),
            ctx.budget().max_item_bytes(),
            ctx.budget().max_proposal_bytes(),
        ) {
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryRepairer),
                "story repair output exceeds a field or collection bound",
            ));
        }
        ctx.replace_story_proposal(proposal)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}
