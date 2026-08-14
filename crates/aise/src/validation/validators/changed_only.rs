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

        for (index, state) in extraction.character_states.iter().enumerate() {
            if let Some(current) = snapshot.character_states().get(&state.character_id) {
                let unchanged = current.location == state.location
                    && current.goals == state.goals
                    && current.attributes == state.attributes;
                if unchanged {
                    issues.push(issue(
                        ValidationIssueCode::UnchangedCharacterEmitted,
                        "character_states",
                        index,
                        "character state is identical to the pre-turn snapshot",
                    ));
                }
            }
        }

        for (index, relationship) in extraction.relationship_states.iter().enumerate() {
            let key = crate::domain::story_instance::state::RelationshipKey {
                source_character_id: relationship.source_character_id.clone(),
                target_character_id: relationship.target_character_id.clone(),
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
