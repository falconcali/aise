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
        let mut issues = Vec::new();
        let Some(proposal) = ctx.proposal() else {
            return Ok(issues);
        };
        let event_count = proposal.events.len();
        for (index, fact) in proposal.world_change.add_facts.iter().enumerate() {
            for (evidence_index, evidence) in fact.evidence.iter().enumerate() {
                let location = Some(ValidationLocation {
                    path: format!("world_change.add_facts[{index}].evidence[{evidence_index}]"),
                    item_index: Some(index as u32),
                });
                match evidence {
                    crate::core::story_proposal::WorldFactEvidenceRef::ProposedEvent { event_index } => {
                        if usize::try_from(*event_index).map(|idx| idx >= event_count).unwrap_or(true) {
                            issues.push(issue(
                                "proposed_event_out_of_range",
                                format!(
                                    "world fact evidence references proposal event {event_index} but the proposal has {event_count} events"
                                ),
                                location,
                            ));
                        }
                    }
                    crate::core::story_proposal::WorldFactEvidenceRef::SnapshotFact(_) => {}
                }
            }
        }
        Ok(issues)
    }
}

fn issue(code: &'static str, message: String, location: Option<ValidationLocation>) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::KnowledgeBoundaryViolated,
        message: format!("{code}: {message}"),
        repairability: Repairability::Fatal,
        location,
    }
}
