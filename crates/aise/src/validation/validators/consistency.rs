use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validation_model::ValidationResult;

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
