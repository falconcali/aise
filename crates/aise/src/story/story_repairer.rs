use crate::config::ContextPreparationConfig;
use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::StoryGeneratorOutput;
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
    pub fn new(gateway: Arc<LlmGateway>, context_config: ContextPreparationConfig) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryRepairerPromptContextProjector::with_context_config(context_config)),
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
        let story_version = ctx.story_version();
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryRepairer,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_repairer",
            story_version,
            issue_count,
            issue_codes,
            "story repairer prompt projected"
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryRepairer);
        let span = tracing::info_span!(
            "story_repairer.repair",
            prompt_profile = "story_repairer",
            story_version,
            issue_count,
        );
        let completion = self
            .gateway
            .complete_text_composed(scope, request, max_output_tokens, LlmCallPurpose::StoryRepair)
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
        let trimmed = completion.text.trim();
        if trimmed.is_empty() {
            tracing::warn!(
                prompt_profile = "story_repairer",
                story_version,
                "story repairer output is trim-empty"
            );
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryRepairer),
                "story repair output is empty".to_owned(),
            ));
        }
        let story_text = BoundedText::try_new(trimmed.to_owned(), "story_text", ctx.budget().max_story_text_bytes())
            .map_err(|error| {
                tracing::warn!(
                    prompt_profile = "story_repairer",
                    story_version,
                    error = %error,
                    "story repairer output exceeds max_story_text_bytes"
                );
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "model_output_invalid",
                    Some(TurnStage::StoryRepairer),
                    format!("story repair output is invalid: {error}"),
                )
            })?;
        let story = StoryGeneratorOutput { story_text };
        tracing::info!(
            prompt_profile = "story_repairer",
            story_version,
            output_bytes = completion.text.len(),
            story_text_bytes = story.story_text.as_str().len(),
            "story repairer output decoded"
        );
        ctx.replace_story(story)
    }
}

fn map_projection_error(error: StoryRepairerProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryRepairerProjectionError::MissingValidation => "missing_validation",
        StoryRepairerProjectionError::ValidationDoesNotRequireRepair => "validation_does_not_require_repair",
        StoryRepairerProjectionError::MissingPreviousStory => "missing_previous_story",
        StoryRepairerProjectionError::EmptyValidationIssues => "empty_validation_issues",
        StoryRepairerProjectionError::PreviousStoryExceedsBounds => "previous_story_exceeds_bounds",
        StoryRepairerProjectionError::Invariant { .. } => "story_repairer_prompt_invariant",
        StoryRepairerProjectionError::GenerationContext(
            crate::story::story_generator_prompt::StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget,
        ) => "required_prompt_data_exceeds_budget",
        StoryRepairerProjectionError::GenerationContext(_) => "story_generator_projection_failed",
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryRepairer),
        error.to_string(),
    )
}
