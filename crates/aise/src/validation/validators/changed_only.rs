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
        let dto = extraction;
        let mut issues = Vec::new();

        for (index, state) in dto.role_states.iter().enumerate() {
            let Ok(role_id) = crate::domain::ids::RoleId::try_new(state.role_id.clone()) else {
                continue;
            };
            if let Some(current) = snapshot.role(&role_id) {
                let location_unchanged = current.state.location.as_str() == state.location;
                let goals_unchanged = current.state.goals.len() == state.goals.len()
                    && current
                        .state
                        .goals
                        .iter()
                        .zip(state.goals.iter())
                        .all(|(existing, incoming)| existing.as_str() == incoming);
                let attributes_unchanged = current.state.attributes.len() == state.attributes.len()
                    && current
                        .state
                        .attributes
                        .iter()
                        .all(|(key, value)| state.attributes.get(key.as_str()) == Some(value));
                if location_unchanged && goals_unchanged && attributes_unchanged {
                    issues.push(issue(
                        ValidationIssueCode::UnchangedRoleEmitted,
                        "role_states",
                        index,
                        "role state is identical to the pre-turn snapshot",
                    ));
                }
            }
        }

        for (index, relationship) in dto.relationship_states.iter().enumerate() {
            let (Ok(source_role_id), Ok(target_role_id), Ok(kind)) = (
                crate::domain::ids::RoleId::try_new(relationship.source_role_id.clone()),
                crate::domain::ids::RoleId::try_new(relationship.target_role_id.clone()),
                crate::domain::asset::ids::RelationshipKind::try_new(relationship.kind.clone()),
            ) else {
                continue;
            };
            let key = crate::domain::story_instance::state::RelationshipKey {
                source_role_id,
                target_role_id,
                kind,
            };
            if let Some(current) = snapshot.relationships().iter().find(|existing| existing.key() == key) {
                if i64::from(current.trust) == relationship.trust {
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
