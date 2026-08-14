use crate::domain::text::estimate_text_tokens;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationRemedy};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct StoryBoundsValidator;

impl DeterministicValidator for StoryBoundsValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(story) = ctx.story() else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();
        let tokens = estimate_text_tokens(story.story_text.as_str());
        if tokens > ctx.budget().max_output_tokens() {
            issues.push(ValidationIssue {
                code: ValidationIssueCode::StoryTextExceedsBounds,
                class: ValidationIssueClass::Story,
                remedy: ValidationRemedy::RepairStory,
                message: format!("story text estimated tokens {tokens} exceed the output budget"),
                location: Some(crate::turn::turn_validation::ValidationLocation {
                    path: "story_text".to_owned(),
                    item_index: None,
                }),
            });
        }
        Ok(issues)
    }
}

#[cfg(test)]
#[path = "tests/story_bounds_tests.rs"]
mod tests;
