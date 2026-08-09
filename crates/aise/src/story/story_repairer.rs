use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_validation::ValidationDecision;
use crate::domain::asset::validation::BoundedText;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{ModelRequest, StoryGeneratorContext, StoryRepairerContext};
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

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let validation = ctx
            .validation()
            .ok_or_else(|| invariant("no validation result before repair"))?;
        if validation.decision() != ValidationDecision::Repair {
            return Err(invariant("repairer only runs when validation requires repair"));
        }
        let issues = validation.issues().to_vec();
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before repair"))?
            .clone();
        let writer_plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before repair"))?
            .clone();
        let previous_proposal = ctx
            .proposal()
            .ok_or_else(|| invariant("proposal not set before repair"))?
            .clone();
        let player_input = BoundedText::try_new(ctx.player_input().to_owned(), "player_input", 4096)
            .map_err(|_| invariant("player input exceeds bound"))?;
        let request = ModelRequest::story_repairer(
            StoryRepairerContext {
                generation: StoryGeneratorContext {
                    baseline,
                    writer_plan,
                    writer_context: ctx.retrieved().writer().to_vec(),
                    character_thoughts: ctx.thoughts().to_vec(),
                    player_input,
                },
                previous_proposal,
                issues,
            },
            ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32,
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
        let proposal =
            serde_json::from_str(&completion.text).map_err(|_| invariant("story proposal output is not valid JSON"))?;
        ctx.replace_story_proposal(proposal)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}
