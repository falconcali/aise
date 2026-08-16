use crate::config::ContextPreparationConfig;
use crate::domain::asset::validation::BoundedText;
use crate::domain::text::estimate_text_tokens;
use crate::prompt::{RuntimePromptVars, TrustedPromptVars};
use crate::story::story_generator_prompt::{
    DefaultStoryGeneratorPromptContextProjector, StoryGeneratorProjectionError, StoryGeneratorPromptContext,
    StoryGeneratorPromptContextProjector, prune_dialogue_examples_to_budget,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_validation::{ValidationDecision, ValidationIssueCode, ValidationLocation};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

pub const STORY_REPAIRER_CSI_SLOT: &str = "context.story_repairer.csi";
pub const STORY_REPAIRER_RC_SLOT: &str = "context.story_repairer.rc";
pub const STORY_REPAIRER_FTI_SLOT: &str = "context.story_repairer.fti";

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairerPromptContext {
    pub generation: StoryGeneratorPromptContext,
    pub previous_story_text: BoundedText,
    pub validation_issues: Vec<StoryRepairValidationIssuePromptView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairValidationIssuePromptView {
    pub code: ValidationIssueCode,
    pub location: Option<StoryRepairValidationLocationPromptView>,
    pub message: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairValidationLocationPromptView {
    pub path: BoundedText,
    pub item_index: Option<u32>,
}

pub struct StoryRepairerPromptProjection {
    pub context: StoryRepairerPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryRepairerProjectionError {
    #[error("story repairer validation result is missing")]
    MissingValidation,
    #[error("story repairer requires ValidationDecision::RepairStory")]
    ValidationDoesNotRequireRepair,
    #[error("story repairer previous story is missing")]
    MissingPreviousStory,
    #[error("story repairer validation issues are empty")]
    EmptyValidationIssues,
    #[error("story repairer previous story exceeds configured bounds")]
    PreviousStoryExceedsBounds,
    #[error("story repairer prompt invariant violated: {code}")]
    Invariant { code: &'static str },
    #[error(transparent)]
    GenerationContext(#[from] StoryGeneratorProjectionError),
}

pub trait StoryRepairerPromptContextProjector: Send + Sync {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryRepairerPromptProjection, StoryRepairerProjectionError>;
}

pub struct DefaultStoryRepairerPromptContextProjector {
    generation_projector: Arc<dyn StoryGeneratorPromptContextProjector>,
}

impl DefaultStoryRepairerPromptContextProjector {
    pub fn new(generation_projector: Arc<dyn StoryGeneratorPromptContextProjector>) -> Self {
        Self { generation_projector }
    }

    pub fn with_context_config(config: ContextPreparationConfig) -> Self {
        Self::new(Arc::new(DefaultStoryGeneratorPromptContextProjector::new(config)))
    }
}

impl Default for DefaultStoryRepairerPromptContextProjector {
    fn default() -> Self {
        Self::with_context_config(ContextPreparationConfig::default())
    }
}

impl StoryRepairerPromptContextProjector for DefaultStoryRepairerPromptContextProjector {
    fn project(
        &self,
        ctx: &TurnExecutionContext,
    ) -> Result<StoryRepairerPromptProjection, StoryRepairerProjectionError> {
        let validation = ctx.validation().ok_or(StoryRepairerProjectionError::MissingValidation)?;
        if validation.decision() != ValidationDecision::RepairStory {
            return Err(StoryRepairerProjectionError::ValidationDoesNotRequireRepair);
        }
        if validation.issues().is_empty() {
            return Err(StoryRepairerProjectionError::EmptyValidationIssues);
        }
        let previous_story = ctx.story().ok_or(StoryRepairerProjectionError::MissingPreviousStory)?.clone();
        if previous_story.story_text.as_str().len() > ctx.budget().max_story_text_bytes() {
            return Err(StoryRepairerProjectionError::PreviousStoryExceedsBounds);
        }
        let validation_issues = validation
            .issues()
            .iter()
            .map(|issue| {
                let message = BoundedText::try_new(
                    issue.message.clone(),
                    "story_repair_validation_message",
                    ctx.budget().max_validation_issue_bytes(),
                )
                .map_err(|_| StoryRepairerProjectionError::Invariant {
                    code: "validation_issue_message_invalid",
                })?;
                let location = issue
                    .location
                    .as_ref()
                    .map(|location| project_location(location, ctx.budget().max_item_bytes()))
                    .transpose()?;
                Ok(StoryRepairValidationIssuePromptView {
                    code: issue.code,
                    location,
                    message,
                })
            })
            .collect::<Result<Vec<_>, StoryRepairerProjectionError>>()?;
        let mut generation = self.generation_projector.project(ctx)?;
        let validation_issues_rendered = render_validation_issues(&validation_issues);
        let previous_story_rendered = quoted(previous_story.story_text.as_str());
        let extra_tokens = estimate_text_tokens(&previous_story_rendered)
            .saturating_add(estimate_text_tokens(&validation_issues_rendered));
        let generation_vars = prune_dialogue_examples_to_budget(
            &mut generation.context,
            ctx.budget().max_context_tokens(),
            extra_tokens,
        )?;
        let mut rc_vars = generation_vars.as_map().clone();
        rc_vars.insert("previous_story_text".into(), Value::String(previous_story_rendered));
        rc_vars.insert("validation_issues".into(), Value::String(validation_issues_rendered));
        Ok(StoryRepairerPromptProjection {
            context: StoryRepairerPromptContext {
                generation: generation.context,
                previous_story_text: previous_story.story_text,
                validation_issues,
            },
            rc_vars: RuntimePromptVars::new(rc_vars),
            fti_vars: generation.fti_vars,
        })
    }
}

fn project_location(
    location: &ValidationLocation,
    max_item_bytes: usize,
) -> Result<StoryRepairValidationLocationPromptView, StoryRepairerProjectionError> {
    let path = BoundedText::try_new(location.path.clone(), "story_repair_validation_location", max_item_bytes)
        .map_err(|_| StoryRepairerProjectionError::Invariant {
            code: "validation_issue_location_invalid",
        })?;
    Ok(StoryRepairValidationLocationPromptView {
        path,
        item_index: location.item_index,
    })
}

fn render_validation_issues(values: &[StoryRepairValidationIssuePromptView]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let location = value
                .location
                .as_ref()
                .map(|location| {
                    let path = quoted(location.path.as_str());
                    match location.item_index {
                        Some(item_index) => format!("{path}\n   Item Index: {item_index}"),
                        None => path,
                    }
                })
                .unwrap_or_else(|| "None.".into());
            format!(
                "{}. Code: {}\n   Location: {}\n   Message: {}",
                index + 1,
                value.code,
                location,
                quoted(value.message.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
#[path = "tests/story_repairer_prompt_tests.rs"]
mod tests;
