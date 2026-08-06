use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct PlayerControlValidator;

impl DeterministicValidator for PlayerControlValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::PlayerControlViolated
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        let Some(baseline) = ctx.baseline() else {
            return Ok(issues);
        };
        let Some(player) = &baseline.player_character else {
            return Ok(issues);
        };
        for (index, memory) in proposal.memory_changes.iter().enumerate() {
            if memory.owner == player.id && memory.kind == crate::domain::memory::MemoryKind::Secret {
                issues.push(issue(
                    "player_secret_memory_overwrite",
                    format!("player character {} cannot be assigned a secret memory", player.id.as_str()),
                    Some(ValidationLocation {
                        path: format!("memory_changes[{index}]"),
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
        code: ValidationIssueCode::PlayerControlViolated,
        message: format!("{code}: {message}"),
        repairability: Repairability::Fatal,
        location,
    }
}
