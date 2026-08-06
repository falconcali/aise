use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct ModificationPermissionValidator;

impl DeterministicValidator for ModificationPermissionValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::ModificationForbidden
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        let Some(baseline) = ctx.baseline() else {
            return Ok(issues);
        };
        let player_character_id = baseline.player_character.as_ref().map(|character| character.id.clone());
        for (index, change) in proposal.character_changes.iter().enumerate() {
            let is_player = player_character_id
                .as_ref()
                .map(|id| *id == change.character_id)
                .unwrap_or(false);
            if is_player {
                continue;
            }
            let is_known = baseline
                .relevant_characters
                .iter()
                .any(|character| character.id == change.character_id);
            if !is_known {
                continue;
            }
            let modifies_internal =
                !change.goal_updates.is_empty() || change.health_delta.is_some() || !change.affinity_deltas.is_empty();
            if modifies_internal {
                issues.push(issue(
                    "non_player_character_modified",
                    format!(
                        "character change modifies non-player character {}",
                        change.character_id.as_str()
                    ),
                    Some(ValidationLocation {
                        path: format!("character_changes[{index}]"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
        }
        Ok(issues)
    }
}

fn issue(code: &'static str, message: String, location: Option<ValidationLocation>) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ModificationForbidden,
        message: format!("{code}: {message}"),
        repairability: Repairability::Fatal,
        location,
    }
}
