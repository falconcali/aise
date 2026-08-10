use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct DomainInvariantValidator;

impl DeterministicValidator for DomainInvariantValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::DomainInvariantViolated
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let (Some(proposal), Some(snapshot)) = (ctx.proposal(), ctx.snapshot()) else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();
        for (index, change) in proposal.relationship_changes.iter().enumerate() {
            let current = snapshot.relationships().iter().find(|relationship| {
                relationship.source_character_id == change.source_character_id
                    && relationship.target_character_id == change.target_character_id
                    && relationship.kind == change.kind
            });
            if current.is_some_and(|relationship| relationship.trust.checked_add(change.trust_delta).is_none()) {
                issues.push(issue("relationship_changes", index, "relationship trust overflows"));
            }
        }
        if proposal
            .scene_change
            .as_ref()
            .is_some_and(|scene| scene.time.as_str().trim().is_empty() || scene.description.as_str().trim().is_empty())
        {
            issues.push(issue("scene_change", 0, "scene time and description must be non-empty"));
        }
        Ok(issues)
    }
}

fn issue(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::DomainInvariantViolated,
        message: message.into(),
        repairability: Repairability::Fatal,
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}
