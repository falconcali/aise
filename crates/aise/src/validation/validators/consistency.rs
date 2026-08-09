use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct ConsistencyValidator;

impl DeterministicValidator for ConsistencyValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::ReferenceMissing
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        let Some(baseline) = ctx.baseline() else {
            return Ok(issues);
        };
        let mut known = BTreeSet::new();
        known.insert(baseline.player_character.character_id.clone());
        for character in &baseline.scene_characters {
            known.insert(character.character_id.clone());
        }
        for entry in &baseline.character_index {
            known.insert(entry.character_id.clone());
        }
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if !known.contains(&change.character_id) {
                issues.push(issue(
                    "unknown_character",
                    format!("character change references unknown character {}", change.character_id.as_str()),
                    Some(ValidationLocation {
                        path: format!("character_changes[{index}].character_id"),
                        item_index: Some(index as u32),
                    }),
                ));
            }
            for affinity in &change.affinity_deltas {
                if !known.contains(&affinity.other) {
                    issues.push(issue(
                        "unknown_affinity_target",
                        format!("affinity delta references unknown character {}", affinity.other.as_str()),
                        Some(ValidationLocation {
                            path: format!("character_changes[{index}].affinity_deltas"),
                            item_index: Some(index as u32),
                        }),
                    ));
                }
            }
        }
        for (index, memory) in proposal.memory_changes.iter().enumerate() {
            if !known.contains(&memory.owner) {
                issues.push(issue(
                    "unknown_memory_owner",
                    format!("memory change references unknown owner {}", memory.owner.as_str()),
                    Some(ValidationLocation {
                        path: format!("memory_changes[{index}].owner"),
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
        code: ValidationIssueCode::ReferenceMissing,
        message: format!("{code}: {message}"),
        repairability: Repairability::Repairable,
        location,
    }
}
