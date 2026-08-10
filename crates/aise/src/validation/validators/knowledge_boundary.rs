use crate::core::story_proposal::{ProposedKnowledgeChange, WorldFactEvidenceRef};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct KnowledgeBoundaryValidator;

impl DeterministicValidator for KnowledgeBoundaryValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::KnowledgeBoundaryViolated
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(proposal) = ctx.proposal() else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();
        for (index, change) in proposal.knowledge_changes.iter().enumerate() {
            if let ProposedKnowledgeChange::Fact { evidence, .. } = change {
                for evidence in evidence {
                    if let WorldFactEvidenceRef::ProposedEvent { event_index } = evidence {
                        if usize::try_from(*event_index).map_or(true, |value| value >= proposal.events.len()) {
                            issues.push(ValidationIssue {
                                code: ValidationIssueCode::KnowledgeBoundaryViolated,
                                message: "fact evidence event index is out of range".into(),
                                repairability: Repairability::Fatal,
                                location: Some(ValidationLocation {
                                    path: format!("knowledge_changes[{index}].evidence"),
                                    item_index: u32::try_from(index).ok(),
                                }),
                            });
                        }
                    }
                }
            }
        }
        Ok(issues)
    }
}
