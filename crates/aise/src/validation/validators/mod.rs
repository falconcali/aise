pub mod consistency;
pub mod domain_invariant;
pub mod knowledge_boundary;
pub mod player_control;
pub mod schema;
pub mod world_fact_evidence;

use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{ValidationIssue, ValidationIssueCode};

pub trait DeterministicValidator: Send + Sync {
    fn code(&self) -> ValidationIssueCode;

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError>;
}
