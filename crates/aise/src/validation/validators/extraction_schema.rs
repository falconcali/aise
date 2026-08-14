use crate::domain::turn::ProposedKnowledgeMutation;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{
    ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationLocation, ValidationRemedy,
};
use crate::validation::validators::DeterministicValidator;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct ExtractionSchemaValidator;

impl DeterministicValidator for ExtractionSchemaValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(extraction) = ctx.extraction() else {
            return Ok(Vec::new());
        };
        let limits = ctx.budget().state_extraction_limits();
        let mut issues = Vec::new();

        if extraction.character_states.len() > limits.max_character_states {
            issues.push(count_issue("character_states", "character state count exceeds its bound"));
        }
        if extraction.relationship_states.len() > limits.max_relationship_states {
            issues.push(count_issue("relationship_states", "relationship state count exceeds its bound"));
        }
        if extraction.knowledge_changes.len() > limits.max_knowledge_changes {
            issues.push(count_issue("knowledge_changes", "knowledge change count exceeds its bound"));
        }

        let mut seen_characters = BTreeSet::new();
        for (index, state) in extraction.character_states.iter().enumerate() {
            if !seen_characters.insert(state.character_id.clone()) {
                issues.push(duplicate_issue(
                    "character_states",
                    index,
                    "duplicate character_id in character_states",
                ));
            }
            if state.goals.len() > limits.max_goals_per_character {
                issues.push(count_issue_at("character_states", index, "character goals exceed their bound"));
            }
            if state.attributes.len() > limits.max_attributes_per_character {
                issues.push(count_issue_at(
                    "character_states",
                    index,
                    "character attributes exceed their bound",
                ));
            }
            for goal in &state.goals {
                if goal.as_str().trim().is_empty() || goal.as_str().len() > limits.max_item_bytes {
                    issues.push(count_issue_at(
                        "character_states",
                        index,
                        "character goal is empty or oversized",
                    ));
                }
            }
        }

        let mut seen_relationships = BTreeSet::new();
        for (index, relationship) in extraction.relationship_states.iter().enumerate() {
            let key = (
                relationship.source_character_id.clone(),
                relationship.target_character_id.clone(),
                relationship.kind.clone(),
            );
            if !seen_relationships.insert(key) {
                issues.push(duplicate_issue(
                    "relationship_states",
                    index,
                    "duplicate relationship key in relationship_states",
                ));
            }
        }

        let mut seen_targets = BTreeSet::new();
        for (index, mutation) in extraction.knowledge_changes.iter().enumerate() {
            match mutation {
                ProposedKnowledgeMutation::Add { value } => {
                    if let Some(issue) = validate_knowledge_value(value, limits, "knowledge_changes", index) {
                        issues.push(issue);
                    }
                }
                ProposedKnowledgeMutation::Update { target, value } => {
                    if !seen_targets.insert(target.as_str().to_owned()) {
                        issues.push(duplicate_issue(
                            "knowledge_changes",
                            index,
                            "duplicate knowledge mutation target",
                        ));
                    }
                    if let Some(issue) = validate_knowledge_value(value, limits, "knowledge_changes", index) {
                        issues.push(issue);
                    }
                }
                ProposedKnowledgeMutation::Delete { target } => {
                    if !seen_targets.insert(target_key(target)) {
                        issues.push(duplicate_issue(
                            "knowledge_changes",
                            index,
                            "duplicate knowledge mutation target",
                        ));
                    }
                }
            }
        }

        let scene = &extraction.current_scene;
        if scene.description.as_str().len() > limits.max_item_bytes {
            issues.push(count_issue("current_scene", "scene description exceeds its bound"));
        }
        if scene.present_character_ids.len() > limits.max_character_states {
            issues.push(count_issue("current_scene", "present character count exceeds its bound"));
        }

        Ok(issues)
    }
}

fn target_key(target: &crate::domain::turn::DeletableKnowledgeId) -> String {
    match target {
        crate::domain::turn::DeletableKnowledgeId::Rumor(id) => format!("rumor:{}", id.as_str()),
        crate::domain::turn::DeletableKnowledgeId::Memory(id) => format!("memory:{}", id.as_str()),
    }
}

fn validate_knowledge_value(
    value: &crate::domain::turn::ProposedKnowledgeValue,
    limits: crate::domain::turn::StoryStateExtractionLimits,
    path: &str,
    index: usize,
) -> Option<ValidationIssue> {
    use crate::domain::turn::ProposedKnowledgeValue;
    let (content, entities, topics) = match value {
        ProposedKnowledgeValue::Fact {
            content,
            entities,
            topics,
            ..
        }
        | ProposedKnowledgeValue::Rumor {
            content,
            entities,
            topics,
            ..
        }
        | ProposedKnowledgeValue::Memory {
            content,
            entities,
            topics,
            ..
        } => (content, entities, topics),
    };
    if content.as_str().trim().is_empty() || content.as_str().len() > limits.max_knowledge_change_bytes {
        return Some(count_issue_at(path, index, "knowledge content is empty or exceeds its bound"));
    }
    if entities.len() > limits.max_entities_per_knowledge {
        return Some(count_issue_at(path, index, "knowledge entities exceed their bound"));
    }
    if topics.len() > limits.max_topics_per_knowledge {
        return Some(count_issue_at(path, index, "knowledge topics exceed their bound"));
    }
    None
}

fn count_issue(path: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ExtractionCountExceeded,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: path.to_owned(),
            item_index: None,
        }),
    }
}

fn count_issue_at(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ExtractionCountExceeded,
        class: ValidationIssueClass::Extraction,
        remedy: ValidationRemedy::ReextractState,
        message: message.to_owned(),
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}

fn duplicate_issue(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ExtractionDuplicateTarget,
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
#[path = "tests/extraction_schema_tests.rs"]
mod tests;
