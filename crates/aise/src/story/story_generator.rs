use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::StoryProposal;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{ModelRequest, StoryGeneratorContext};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
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

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before story generation"))?
            .clone();
        let writer_plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before story generation"))?
            .clone();
        let writer_context = ctx.retrieved().writer().to_vec();
        let character_thoughts = ctx.thoughts().to_vec();
        let player_input = BoundedText::try_new(ctx.player_input().to_owned(), "player_input", 4096)
            .map_err(|_| invariant("player input exceeds bound"))?;
        let request = ModelRequest::story_generator(
            StoryGeneratorContext {
                baseline,
                writer_plan,
                writer_context,
                character_thoughts,
                player_input,
            },
            ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32,
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let completion = self.gateway.complete_typed(scope, request).await.map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "llm_error",
                Some(TurnStage::StoryGenerator),
                error.to_string(),
            )
        })?;
        let proposal: StoryProposal = serde_json::from_str(&completion.text).map_err(|_| {
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryGenerator),
                "story proposal output is invalid",
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
                Some(TurnStage::StoryGenerator),
                "story proposal output exceeds a field or collection bound",
            ));
        }
        ctx.set_story_proposal(proposal)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}
