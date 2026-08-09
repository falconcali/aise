use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;
use std::collections::BTreeSet;

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
        let player_character_id = baseline.player_character.character_id.clone();
        let mut known = BTreeSet::new();
        known.insert(player_character_id.clone());
        for character in &baseline.scene_characters {
            known.insert(character.character_id.clone());
        }
        for entry in &baseline.character_index {
            known.insert(entry.character_id.clone());
        }
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if change.character_id == player_character_id {
                continue;
            }
            if !known.contains(&change.character_id) {
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
