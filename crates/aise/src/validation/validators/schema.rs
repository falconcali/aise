use crate::error::AiseError;
use crate::runtime::turn_execution_ctx::TurnExecutionContext;
use crate::validation::validation_model::ValidationResult;

/// Deterministic checks that never call an LLM: schema, data legality, and
/// state-modification permissions.
#[derive(Default)]
pub struct SchemaValidator;

impl SchemaValidator {
    pub fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        // Framework stub: structural checks on ctx.draft.
        let _ = ctx;
        Ok(ValidationResult::default())
    }
}
