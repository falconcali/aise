use crate::core::story_proposal::{ProposedKnowledgeChange, WorldFactEvidenceRef};
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::domain::knowledge::KnowledgeSourceId;
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct WorldFactEvidenceValidator;

impl DeterministicValidator for WorldFactEvidenceValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::WorldFactEvidenceMissing
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(proposal) = ctx.proposal() else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();
        for (index, change) in proposal.knowledge_changes.iter().enumerate() {
            let ProposedKnowledgeChange::Fact { evidence, .. } = change else {
                continue;
            };
            if evidence.is_empty() {
                issues.push(issue(index, ValidationIssueCode::WorldFactEvidenceMissing));
            }
            for evidence in evidence {
                if let WorldFactEvidenceRef::SnapshotFact(id) = evidence {
                    let available = ctx
                        .retrieved()
                        .writer()
                        .iter()
                        .any(|item| item.provenance.source_id == KnowledgeSourceId::Fact(id.clone()));
                    if !available {
                        issues.push(issue(index, ValidationIssueCode::WorldFactEvidenceInvalid));
                    }
                }
            }
        }
        Ok(issues)
    }
}

fn issue(index: usize, code: ValidationIssueCode) -> ValidationIssue {
    ValidationIssue {
        code,
        message: "fact evidence is missing or unavailable".into(),
        repairability: Repairability::Fatal,
        location: Some(ValidationLocation {
            path: format!("knowledge_changes[{index}].evidence"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}
