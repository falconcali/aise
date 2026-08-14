pub mod changed_only;
pub mod domain_invariant;
pub mod extraction_schema;
pub mod reference;
pub mod story_bounds;
pub mod story_state_consistency;

use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::ValidationIssue;

pub trait DeterministicValidator: Send + Sync {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError>;
}
