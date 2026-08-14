use crate::domain::turn::StoryStateExtractorOutput;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::story::story_state_extractor_prompt::{
    DefaultStoryStateExtractorPromptContextProjector, StoryStateExtractorProjectionError,
    StoryStateExtractorPromptContextProjector,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::{LlmCallPurpose, TurnPhase};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_validation::{
    BoundedValidationIssues, ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationRemedy,
};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::Instrument;

pub struct StoryStateExtractor {
    gateway: Arc<LlmGateway>,
    projector: Arc<dyn StoryStateExtractorPromptContextProjector>,
}

impl StoryStateExtractor {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryStateExtractorPromptContextProjector),
        }
    }

    pub fn with_projector(
        gateway: Arc<LlmGateway>,
        projector: Arc<dyn StoryStateExtractorPromptContextProjector>,
    ) -> Self {
        Self { gateway, projector }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryStateExtractor {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryStateExtractor
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let is_reextraction = ctx.phase() == TurnPhase::StateReextractionRequired;
        let projection = self.projector.project(ctx).map_err(map_projection_error)?;
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryStateExtractor,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx
            .budget()
            .state_extractor_max_output_tokens()
            .min(ctx.budget().remaining_output_tokens())
            .min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_state_extractor",
            is_reextraction,
            "story state extractor prompt projected"
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryStateExtractor);
        let span = tracing::info_span!(
            "story_state_extractor.extract",
            prompt_profile = "story_state_extractor",
            is_reextraction
        );
        let completion = self
            .gateway
            .complete_composed(scope, request, max_output_tokens, LlmCallPurpose::StoryStateExtraction)
            .instrument(span)
            .await
            .map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "llm_error",
                    Some(TurnStage::StoryStateExtractor),
                    error.to_string(),
                )
            })?;
        match serde_json::from_str::<StoryStateExtractorOutput>(&completion.text) {
            Ok(output) => {
                tracing::info!(
                    prompt_profile = "story_state_extractor",
                    is_reextraction,
                    output_bytes = completion.text.len(),
                    character_state_count = output.character_states.len(),
                    relationship_state_count = output.relationship_states.len(),
                    knowledge_change_count = output.knowledge_changes.len(),
                    "story state extractor output decoded"
                );
                if is_reextraction {
                    ctx.replace_state_extraction(output)
                } else {
                    ctx.set_state_extraction(output)
                }
            }
            Err(error) => {
                tracing::warn!(
                    prompt_profile = "story_state_extractor",
                    is_reextraction,
                    error = %error,
                    "story state extractor output decode failed"
                );
                let issue = ValidationIssue {
                    code: ValidationIssueCode::ExtractionSchemaInvalid,
                    class: ValidationIssueClass::Extraction,
                    remedy: ValidationRemedy::ReextractState,
                    message: format!("state extraction output is invalid: {error}"),
                    location: None,
                };
                let issues = BoundedValidationIssues::try_new(vec![issue], ctx.budget().max_validation_issues())?;
                ctx.record_state_extraction_failure(issues)
            }
        }
    }
}

fn map_projection_error(error: StoryStateExtractorProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryStateExtractorProjectionError::MissingStory => "missing_story",
        StoryStateExtractorProjectionError::MissingSnapshot => "missing_snapshot",
        StoryStateExtractorProjectionError::ValidationDoesNotRequireReextraction => {
            "validation_does_not_require_reextraction"
        }
        StoryStateExtractorProjectionError::EmptyValidationIssues => "empty_validation_issues",
        StoryStateExtractorProjectionError::Invariant { .. } => "story_state_extractor_prompt_invariant",
        StoryStateExtractorProjectionError::RequiredPromptDataExceedsBudget => {
            "story_state_extractor_prompt_budget_exceeded"
        }
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryStateExtractor),
        error.to_string(),
    )
}
