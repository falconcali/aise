use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{
    ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationLocation, ValidationRemedy,
};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct ChangedOnlyValidator;

impl DeterministicValidator for ChangedOnlyValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let (Some(extraction), Some(snapshot)) = (ctx.extraction(), ctx.snapshot()) else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();

        for (index, state) in extraction.role_states.iter().enumerate() {
            if let Some(current) = snapshot.role(&state.role_id) {
                let unchanged = current.state.location == state.location
                    && current.state.goals == state.goals
                    && current.state.attributes == state.attributes;
                if unchanged {
                    issues.push(issue(
                        ValidationIssueCode::UnchangedRoleEmitted,
                        "role_states",
                        index,
                        "role state is identical to the pre-turn snapshot",
                    ));
                }
            }
        }

        for (index, relationship) in extraction.relationship_states.iter().enumerate() {
            let key = crate::domain::story_instance::state::RelationshipKey {
                source_role_id: relationship.source_role_id.clone(),
                target_role_id: relationship.target_role_id.clone(),
                kind: relationship.kind.clone(),
            };
            if let Some(current) = snapshot.relationships().iter().find(|existing| existing.key() == key) {
                if current.trust == relationship.trust {
                    issues.push(issue(
                        ValidationIssueCode::UnchangedRelationshipEmitted,
                        "relationship_states",
                        index,
                        "relationship state is identical to the pre-turn snapshot",
                    ));
                }
            }
        }

        Ok(issues)
    }
}

fn issue(code: ValidationIssueCode, path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}

#[cfg(test)]
#[path = "tests/changed_only_tests.rs"]
mod tests;
