use crate::domain::knowledge::hint::RetrievalHint;
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
        let dto = extraction;
        let limits = ctx.budget().state_extraction_limits();
        let mut issues = Vec::new();

        if dto.new_roles.len() > limits.max_new_roles {
            issues.push(count_issue("new_roles", "new role count exceeds its bound"));
        }
        if dto.role_states.len() > limits.max_role_states {
            issues.push(count_issue("role_states", "role state count exceeds its bound"));
        }
        if dto.relationship_states.len() > limits.max_relationship_states {
            issues.push(count_issue("relationship_states", "relationship state count exceeds its bound"));
        }
        for path in [
            "add_facts",
            "update_facts",
            "add_rumors",
            "update_rumors",
            "delete_rumor_ids",
            "add_memories",
            "update_memories",
            "delete_memory_ids",
        ] {
            let len = match path {
                "add_facts" => dto.add_facts.len(),
                "update_facts" => dto.update_facts.len(),
                "add_rumors" => dto.add_rumors.len(),
                "update_rumors" => dto.update_rumors.len(),
                "delete_rumor_ids" => dto.delete_rumor_ids.len(),
                "add_memories" => dto.add_memories.len(),
                "update_memories" => dto.update_memories.len(),
                _ => dto.delete_memory_ids.len(),
            };
            if len > limits.max_knowledge_items {
                issues.push(count_issue(path, "knowledge change count exceeds its bound"));
            }
        }
        if dto.narrative_condition_judgments.len() > limits.max_condition_queries {
            issues.push(count_issue(
                "narrative_condition_judgments",
                "narrative condition judgment count exceeds its bound",
            ));
        }
        if dto.cast_policy_violations.len() > limits.max_cast_policy_violations {
            issues.push(count_issue(
                "cast_policy_violations",
                "cast policy violation count exceeds its bound",
            ));
        }

        let mut seen_role_ids = BTreeSet::new();
        for (index, role) in dto.new_roles.iter().enumerate() {
            if !seen_role_ids.insert(role.role_id.clone()) {
                issues.push(duplicate_issue("new_roles", index, "duplicate role_id in new_roles"));
            }
            if role.name.trim().is_empty() || role.name.len() > limits.max_role_profile_bytes {
                issues.push(count_issue_at("new_roles", index, "name is empty or exceeds its bound"));
            }
            if role.narrative_function.trim().is_empty()
                || role.narrative_function.len() > limits.max_role_profile_bytes
            {
                issues.push(count_issue_at(
                    "new_roles",
                    index,
                    "narrative_function is empty or exceeds its bound",
                ));
            }
            for field in [
                &role.role_label,
                &role.background,
                &role.appearance,
                &role.personality,
                &role.speaking_style,
            ] {
                if field.len() > limits.max_role_profile_bytes {
                    issues.push(count_issue_at("new_roles", index, "role profile field exceeds its bound"));
                }
            }
            if role.goals.len() > limits.max_goals_per_role {
                issues.push(count_issue_at("new_roles", index, "role goals exceed their bound"));
            }
            if role.attributes.len() > limits.max_attributes_per_role {
                issues.push(count_issue_at("new_roles", index, "role attributes exceed their bound"));
            }
        }
        for (index, state) in dto.role_states.iter().enumerate() {
            if seen_role_ids.contains(&state.role_id) {
                issues.push(duplicate_issue("role_states", index, "role_id also appears in new_roles"));
            }
            if !seen_role_ids.insert(state.role_id.clone()) {
                issues.push(duplicate_issue("role_states", index, "duplicate role_id in role_states"));
            }
            if state.goals.len() > limits.max_goals_per_role {
                issues.push(count_issue_at("role_states", index, "role goals exceed their bound"));
            }
            if state.attributes.len() > limits.max_attributes_per_role {
                issues.push(count_issue_at("role_states", index, "role attributes exceed their bound"));
            }
            for goal in &state.goals {
                if goal.trim().is_empty() || goal.len() > limits.max_item_bytes {
                    issues.push(count_issue_at("role_states", index, "role goal is empty or oversized"));
                }
            }
        }

        let mut seen_relationships = BTreeSet::new();
        for (index, relationship) in dto.relationship_states.iter().enumerate() {
            let key = (
                relationship.source_role_id.clone(),
                relationship.target_role_id.clone(),
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

        for (index, draft) in dto.add_facts.iter().enumerate() {
            validate_content(&draft.content, &draft.retrieval_hint, limits, "add_facts", index, &mut issues);
        }
        let mut seen_fact_targets = BTreeSet::new();
        for (index, update) in dto.update_facts.iter().enumerate() {
            if !seen_fact_targets.insert(update.id.clone()) {
                issues.push(duplicate_issue("update_facts", index, "duplicate update_facts target"));
            }
            validate_content(
                &update.content,
                &update.retrieval_hint,
                limits,
                "update_facts",
                index,
                &mut issues,
            );
        }
        for (index, draft) in dto.add_rumors.iter().enumerate() {
            validate_content(&draft.content, &draft.retrieval_hint, limits, "add_rumors", index, &mut issues);
        }
        let mut seen_rumor_targets = BTreeSet::new();
        for (index, update) in dto.update_rumors.iter().enumerate() {
            if !seen_rumor_targets.insert(update.id.clone()) {
                issues.push(duplicate_issue("update_rumors", index, "duplicate update_rumors target"));
            }
            validate_content(
                &update.content,
                &update.retrieval_hint,
                limits,
                "update_rumors",
                index,
                &mut issues,
            );
        }
        for (index, raw_id) in dto.delete_rumor_ids.iter().enumerate() {
            if !seen_rumor_targets.insert(raw_id.clone()) {
                issues.push(duplicate_issue("delete_rumor_ids", index, "duplicate rumor mutation target"));
            }
        }
        for (index, draft) in dto.add_memories.iter().enumerate() {
            if draft.content.trim().is_empty() || draft.content.len() > limits.max_knowledge_change_bytes {
                issues.push(count_issue_at(
                    "add_memories",
                    index,
                    "memory content is empty or exceeds its bound",
                ));
            }
        }
        let mut seen_memory_targets = BTreeSet::new();
        for (index, update) in dto.update_memories.iter().enumerate() {
            if !seen_memory_targets.insert(update.id.clone()) {
                issues.push(duplicate_issue("update_memories", index, "duplicate update_memories target"));
            }
            if update.content.trim().is_empty() || update.content.len() > limits.max_knowledge_change_bytes {
                issues.push(count_issue_at(
                    "update_memories",
                    index,
                    "memory content is empty or exceeds its bound",
                ));
            }
        }
        for (index, raw_id) in dto.delete_memory_ids.iter().enumerate() {
            if !seen_memory_targets.insert(raw_id.clone()) {
                issues.push(duplicate_issue("delete_memory_ids", index, "duplicate memory mutation target"));
            }
        }

        for (index, judgment) in dto.narrative_condition_judgments.iter().enumerate() {
            if crate::domain::asset::ids::NarrativeConditionKey::try_new(judgment.condition_key.clone()).is_err() {
                issues.push(count_issue_at(
                    "narrative_condition_judgments",
                    index,
                    "condition_key does not resolve to a valid identifier",
                ));
            }
            if judgment.evidence.len() > limits.max_condition_evidence_bytes {
                issues.push(count_issue_at(
                    "narrative_condition_judgments",
                    index,
                    "evidence exceeds its bound",
                ));
            }
            if judgment.reason.len() > limits.max_condition_reason_bytes {
                issues.push(count_issue_at(
                    "narrative_condition_judgments",
                    index,
                    "reason exceeds its bound",
                ));
            }
        }

        for (index, violation) in dto.cast_policy_violations.iter().enumerate() {
            if violation.trim().is_empty() {
                issues.push(count_issue_at(
                    "cast_policy_violations",
                    index,
                    "violation reason must not be empty",
                ));
            }
        }

        Ok(issues)
    }
}

fn validate_content(
    content: &str,
    retrieval_hint: &str,
    limits: crate::domain::turn::StoryStateExtractionLimits,
    path: &str,
    index: usize,
    issues: &mut Vec<ValidationIssue>,
) {
    if content.trim().is_empty() || content.len() > limits.max_knowledge_change_bytes {
        issues.push(count_issue_at(path, index, "knowledge content is empty or exceeds its bound"));
    }
    if !retrieval_hint.is_empty() && retrieval_hint.len() > RetrievalHint::MAX_BYTES {
        issues.push(count_issue_at(path, index, "retrieval_hint exceeds its bound"));
    }
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
