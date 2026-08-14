use crate::domain::knowledge::KnowledgeKind;
use crate::domain::turn::{DeletableKnowledgeId, ProposedKnowledgeMutation, ProposedKnowledgeValue};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{
    ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationLocation, ValidationRemedy,
};
use crate::validation::validators::DeterministicValidator;
use crate::validation::validators::reference::modifiable_knowledge_index;

#[derive(Default)]
pub struct DomainInvariantValidator;

impl DeterministicValidator for DomainInvariantValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(extraction) = ctx.extraction() else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();

        let scene = &extraction.current_scene;
        if scene.time.as_str().trim().is_empty() || scene.description.as_str().trim().is_empty() {
            issues.push(issue("current_scene", 0, "scene time and description must be non-empty"));
        }

        let modifiable = modifiable_knowledge_index(ctx);
        for (index, mutation) in extraction.knowledge_changes.iter().enumerate() {
            match mutation {
                ProposedKnowledgeMutation::Add { .. } => {}
                ProposedKnowledgeMutation::Update { target, value } => {
                    let value_kind = match value {
                        ProposedKnowledgeValue::Fact { .. } => KnowledgeKind::Fact,
                        ProposedKnowledgeValue::Rumor { .. } => KnowledgeKind::Rumor,
                        ProposedKnowledgeValue::Memory { .. } => KnowledgeKind::Memory,
                    };
                    if value_kind == KnowledgeKind::Fact
                        && !matches!(target, crate::domain::knowledge::KnowledgeSourceId::Fact(_))
                    {
                        issues.push(issue("knowledge_changes", index, "update target/value kind mismatch"));
                    }
                }
                ProposedKnowledgeMutation::Delete { target } => {
                    let kind = match target {
                        DeletableKnowledgeId::Rumor(_) => KnowledgeKind::Rumor,
                        DeletableKnowledgeId::Memory(_) => KnowledgeKind::Memory,
                    };
                    let source_id = match target {
                        DeletableKnowledgeId::Rumor(id) => {
                            crate::domain::knowledge::KnowledgeSourceId::Rumor(id.clone())
                        }
                        DeletableKnowledgeId::Memory(id) => {
                            crate::domain::knowledge::KnowledgeSourceId::Memory(id.clone())
                        }
                    };
                    if modifiable.get(&source_id).is_some_and(|existing| *existing != kind) {
                        issues.push(issue("knowledge_changes", index, "delete target kind mismatch"));
                    }
                }
            }
        }

        Ok(issues)
    }
}

fn issue(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::DomainInvariantViolated,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}

#[cfg(test)]
#[path = "tests/domain_invariant_tests.rs"]
mod tests;
