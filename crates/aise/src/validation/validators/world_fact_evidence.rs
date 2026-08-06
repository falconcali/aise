use crate::core::story_proposal::WorldFactEvidenceRef;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::validation::validators::DeterministicValidator;

#[derive(Default)]
pub struct WorldFactEvidenceValidator;

impl DeterministicValidator for WorldFactEvidenceValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::WorldFactEvidenceMissing
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        let snapshot_fact_ids: Vec<String> = ctx
            .snapshot()
            .and_then(|snapshot| snapshot.world())
            .map(|world| world.facts.iter().map(|fact| fact.id.as_str().to_owned()).collect())
            .unwrap_or_default();
        for (index, fact) in proposal.world_change.add_facts.iter().enumerate() {
            let location = Some(ValidationLocation {
                path: format!("world_change.add_facts[{index}]"),
                item_index: Some(index as u32),
            });
            if fact.evidence.is_empty() {
                issues.push(ValidationIssue {
                    code: ValidationIssueCode::WorldFactEvidenceMissing,
                    message: "world fact carries no evidence reference".into(),
                    repairability: Repairability::Fatal,
                    location: location.clone(),
                });
                continue;
            }
            for (evidence_index, evidence) in fact.evidence.iter().enumerate() {
                match evidence {
                    WorldFactEvidenceRef::SnapshotFact(fact_id) => {
                        if !snapshot_fact_ids.iter().any(|id| id == fact_id.as_str()) {
                            issues.push(ValidationIssue {
                                code: ValidationIssueCode::WorldFactEvidenceInvalid,
                                message: format!(
                                    "world fact evidence references unknown snapshot fact {}",
                                    fact_id.as_str()
                                ),
                                repairability: Repairability::Fatal,
                                location: Some(ValidationLocation {
                                    path: format!("world_change.add_facts[{index}].evidence[{evidence_index}]"),
                                    item_index: Some(index as u32),
                                }),
                            });
                        }
                    }
                    WorldFactEvidenceRef::ProposedEvent { .. } => {}
                }
            }
        }
        Ok(issues)
    }
}
