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
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if let Some(delta) = change.health_delta {
                if delta < 0 {
                    let target_health = ctx
                        .snapshot()
                        .and_then(|snapshot| {
                            snapshot
                                .characters()
                                .iter()
                                .find(|character| character.id == change.character_id)
                        })
                        .map(|character| character.internal_state.health)
                        .unwrap_or(0);
                    let projected = target_health.saturating_add(delta);
                    if projected < 0 {
                        issues.push(issue(
                            "negative_health",
                            format!(
                                "character {} health would become negative ({projected})",
                                change.character_id.as_str()
                            ),
                            Some(ValidationLocation {
                                path: format!("character_changes[{index}].health_delta"),
                                item_index: Some(index as u32),
                            }),
                        ));
                    }
                }
            }
        }
        Ok(issues)
    }
}

fn issue(code: &'static str, message: String, location: Option<ValidationLocation>) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::DomainInvariantViolated,
        message: format!("{code}: {message}"),
        repairability: Repairability::Fatal,
        location,
    }
}
