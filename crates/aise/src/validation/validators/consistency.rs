use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_validation::{ValidationResult, repairable};
use crate::domain::ids::CharacterId;
use crate::error::AiseError;

#[derive(Default)]
pub struct ConsistencyValidator;

impl ConsistencyValidator {
    pub async fn validate(&self, ctx: &TurnExecutionContext) -> Result<ValidationResult, AiseError> {
        let proposal = ctx
            .proposal()
            .ok_or_else(|| AiseError::InvariantViolation("no story proposal before consistency validation".into()))?;
        let baseline = ctx
            .baseline()
            .ok_or_else(|| AiseError::InvariantViolation("no baseline context before consistency validation".into()))?;
        let known: Vec<&CharacterId> = baseline.relevant_characters.iter().map(|character| &character.id).collect();
        let mut issues = Vec::new();
        for change in &proposal.character_changes {
            if !known.contains(&&change.character_id) {
                issues.push(repairable(
                    "unknown_character",
                    format!("character change references unknown character {}", change.character_id.as_str()),
                ));
            }
            for affinity in &change.affinity_deltas {
                if !known.contains(&&affinity.other) {
                    issues.push(repairable(
                        "unknown_affinity_target",
                        format!("affinity delta references unknown character {}", affinity.other.as_str()),
                    ));
                }
            }
        }
        for memory in &proposal.memory_changes {
            if !known.contains(&&memory.owner) {
                issues.push(repairable(
                    "unknown_memory_owner",
                    format!("memory change references unknown owner {}", memory.owner.as_str()),
                ));
            }
        }
        if issues.is_empty() {
            return Ok(ValidationResult::pass());
        }
        let first = issues.remove(0);
        let mut result = ValidationResult::repair(&first.code, first.message);
        for issue in issues {
            result = result.with_issue(issue);
        }
        Ok(result)
    }
}
