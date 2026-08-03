use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validation_model::ValidationResult;

#[derive(Default)]
pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        let _ = ctx;
        Ok(ValidationResult::default())
    }
}
