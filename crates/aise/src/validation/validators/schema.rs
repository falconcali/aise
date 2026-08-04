use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_validation::{ValidationResult, fatal};
use crate::error::AiseError;

#[derive(Default)]
pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        let proposal = match ctx.proposal() {
            Some(p) => p,
            None => {
                return Ok(ValidationResult {
                    pass: false,
                    issues: vec![fatal("missing_proposal", "no story proposal produced")],
                });
            }
        };
        if proposal.story_text.trim().is_empty() {
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
