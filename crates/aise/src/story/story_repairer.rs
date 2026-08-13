use crate::domain::turn::StoryProposal;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::story::story_repairer_prompt::{
    DefaultStoryRepairerPromptContextProjector, StoryRepairerProjectionError, StoryRepairerPromptContextProjector,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::LlmCallPurpose;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::Instrument;

pub struct StoryRepairer {
    gateway: Arc<LlmGateway>,
    projector: Arc<dyn StoryRepairerPromptContextProjector>,
}

impl StoryRepairer {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryRepairerPromptContextProjector::default()),
        }
    }

    pub fn with_projector(gateway: Arc<LlmGateway>, projector: Arc<dyn StoryRepairerPromptContextProjector>) -> Self {
        Self { gateway, projector }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryRepairer {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryRepairer
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let projection = self.projector.project(ctx).map_err(map_projection_error)?;
        let issue_count = projection.context.validation_issues.len();
        let issue_codes = projection
            .context
            .validation_issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let proposal_revision = ctx.proposal_revision();
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryRepairer,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_repairer",
            proposal_revision,
            issue_count,
            issue_codes,
            "story repairer prompt projected"
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryRepairer);
        let span = tracing::info_span!(
            "story_repairer.repair",
            prompt_profile = "story_repairer",
            proposal_revision,
            issue_count,
        );
        let completion = self
            .gateway
            .complete_composed(scope, request, max_output_tokens, LlmCallPurpose::StoryRepair)
            .instrument(span)
            .await
            .map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "llm_error",
                    Some(TurnStage::StoryRepairer),
                    error.to_string(),
                )
            })?;
        let proposal: StoryProposal = serde_json::from_str(&completion.text).map_err(|error| {
            tracing::warn!(
                prompt_profile = "story_repairer",
                proposal_revision,
                error = %error,
                "story repairer proposal decode failed"
            );
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryRepairer),
                format!("story repair output is invalid: {error}"),
            )
        })?;
        if !proposal.is_within_bounds(
            ctx.budget().max_total_items(),
            ctx.budget().max_item_bytes(),
            ctx.budget().max_proposal_bytes(),
        ) {
            tracing::warn!(
                prompt_profile = "story_repairer",
                proposal_revision,
                output_bytes = completion.text.len(),
                "story repairer proposal rejected"
            );
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryRepairer),
                "story repair output exceeds a field or collection bound",
            ));
        }
        tracing::info!(
            prompt_profile = "story_repairer",
            proposal_revision,
            output_bytes = completion.text.len(),
            event_count = proposal.events.len(),
            character_change_count = proposal.character_changes.len(),
            relationship_change_count = proposal.relationship_changes.len(),
            knowledge_change_count = proposal.knowledge_changes.len(),
            perception_count = proposal.perceptions.len(),
            "story repairer proposal decoded"
        );
        ctx.replace_story_proposal(proposal)
    }
}

fn map_projection_error(error: StoryRepairerProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryRepairerProjectionError::MissingValidation => "missing_validation",
        StoryRepairerProjectionError::ValidationDoesNotRequireRepair => "validation_does_not_require_repair",
        StoryRepairerProjectionError::MissingPreviousProposal => "missing_previous_proposal",
        StoryRepairerProjectionError::EmptyValidationIssues => "empty_validation_issues",
        StoryRepairerProjectionError::FatalValidationIssue => "fatal_validation_issue",
        StoryRepairerProjectionError::PreviousProposalExceedsBounds => "previous_proposal_exceeds_bounds",
        StoryRepairerProjectionError::Invariant { .. } => "story_repairer_prompt_invariant",
        StoryRepairerProjectionError::GenerationContext(_) => "story_generator_projection_failed",
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryRepairer),
        error.to_string(),
    )
}
