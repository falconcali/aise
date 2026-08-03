use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validation_model::{ValidationResult, fatal};

#[derive(Default)]
pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        let draft = match &ctx.draft {
            Some(d) => d,
            None => {
                return Ok(ValidationResult {
                    pass: false,
                    issues: vec![fatal("missing_draft", "no story draft produced")],
                });
            }
        };
        if draft.story_text.trim().is_empty() {
            return Ok(ValidationResult {
                pass: false,
                issues: vec![fatal("empty_story", "story text is empty")],
            });
        }
        Ok(ValidationResult {
            pass: true,
            issues: Vec::new(),
        })
    }
}
