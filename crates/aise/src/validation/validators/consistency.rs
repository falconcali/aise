use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_validation::ValidationResult;
use crate::error::AiseError;

#[derive(Default)]
pub struct ConsistencyValidator;

impl ConsistencyValidator {
    pub async fn validate(&self, _ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        Ok(ValidationResult {
            pass: true,
            issues: Vec::new(),
        })
    }
}
