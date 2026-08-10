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
        let (Some(proposal), Some(snapshot)) = (ctx.proposal(), ctx.snapshot()) else {
            return Ok(Vec::new());
        };
        let player = snapshot
            .role_bindings()
            .values()
            .find(|binding| binding.is_player_controlled())
            .map(|binding| &binding.character_id);
        Ok(proposal
            .character_changes
            .iter()
            .enumerate()
            .filter(|(_, change)| player == Some(&change.character_id))
            .map(|(index, _)| ValidationIssue {
                code: ValidationIssueCode::PlayerControlViolated,
                message: "model cannot mutate player-controlled character state".into(),
                repairability: Repairability::Fatal,
                location: Some(ValidationLocation {
                    path: format!("character_changes[{index}]"),
                    item_index: u32::try_from(index).ok(),
                }),
            })
            .collect())
    }
}
