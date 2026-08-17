use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::LocationKey;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{DeletableKnowledgeId, ProposedKnowledgeMutation, ProposedKnowledgeValue};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::TurnExecutionError;
use crate::turn::turn_validation::{
    ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationLocation, ValidationRemedy,
};
use crate::validation::validators::DeterministicValidator;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct ReferenceValidator;

impl DeterministicValidator for ReferenceValidator {
    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let (Some(extraction), Some(snapshot)) = (ctx.extraction(), ctx.snapshot()) else {
            return Ok(Vec::new());
        };
        let known_roles = snapshot.roles().keys().cloned().collect::<BTreeSet<_>>();
        let modifiable = modifiable_knowledge_index(ctx);
        let mut issues = Vec::new();

        for (index, state) in extraction.role_states.iter().enumerate() {
            if !known_roles.contains(&state.role_id) {
                issues.push(issue("role_states", index, "role_id is not a known role"));
            }
            if !location_key_resolves(&state.location, snapshot) {
                issues.push(issue("role_states", index, "location does not resolve to a known location"));
            }
        }

        for (index, relationship) in extraction.relationship_states.iter().enumerate() {
            if !known_roles.contains(&relationship.source_role_id)
                || !known_roles.contains(&relationship.target_role_id)
            {
                issues.push(issue("relationship_states", index, "relationship references an unknown role"));
            }
        }

        for (index, mutation) in extraction.knowledge_changes.iter().enumerate() {
            match mutation {
                ProposedKnowledgeMutation::Add { value } => {
                    if let Some(message) = invalid_value_reference(value, &known_roles, snapshot) {
                        issues.push(issue("knowledge_changes", index, message));
                    }
                }
                ProposedKnowledgeMutation::Update { target, value } => {
                    match modifiable.get(target) {
                        Some(kind) if *kind == value_kind(value) => {}
                        Some(_) => issues.push(issue("knowledge_changes", index, "update target kind mismatch")),
                        None => issues.push(issue("knowledge_changes", index, "update target is not modifiable")),
                    }
                    if let Some(message) = invalid_value_reference(value, &known_roles, snapshot) {
                        issues.push(issue("knowledge_changes", index, message));
                    }
                }
                ProposedKnowledgeMutation::Delete { target } => {
                    let source_id = deletable_source_id(target);
                    if !modifiable.contains_key(&source_id) {
                        issues.push(issue("knowledge_changes", index, "delete target is not modifiable"));
                    }
                }
            }
        }

        Ok(issues)
    }
}

fn invalid_value_reference(
    value: &ProposedKnowledgeValue,
    known_roles: &BTreeSet<crate::domain::ids::RoleId>,
    snapshot: &StoryReadSnapshot,
) -> Option<&'static str> {
    let (entities, topics, memory_owner, rumor_source) = match value {
        ProposedKnowledgeValue::Fact { entities, topics, .. } => (entities, topics, None, None),
        ProposedKnowledgeValue::Rumor {
            entities,
            topics,
            source_role_id,
            ..
        } => (entities, topics, None, source_role_id.as_ref()),
        ProposedKnowledgeValue::Memory {
            entities,
            topics,
            owner,
            ..
        } => (entities, topics, Some(owner), None),
    };
    if memory_owner.is_some_and(|owner| !known_roles.contains(owner)) {
        return Some("memory owner is not a known role");
    }
    if rumor_source.is_some_and(|role| !known_roles.contains(role)) {
        return Some("rumor source_role_id is not a known role");
    }
    if entities.iter().any(|entity| !entity_resolves(entity, snapshot)) {
        return Some("knowledge entity does not resolve");
    }
    if topics.iter().any(|topic| !snapshot.topic_dictionary().contains_key(topic)) {
        return Some("knowledge topic does not resolve");
    }
    if has_duplicates(entities) || has_duplicates(topics) {
        return Some("knowledge entities or topics contain duplicates");
    }
    None
}

fn value_kind(value: &ProposedKnowledgeValue) -> KnowledgeKind {
    match value {
        ProposedKnowledgeValue::Fact { .. } => KnowledgeKind::Fact,
        ProposedKnowledgeValue::Rumor { .. } => KnowledgeKind::Rumor,
        ProposedKnowledgeValue::Memory { .. } => KnowledgeKind::Memory,
    }
}

fn deletable_source_id(target: &DeletableKnowledgeId) -> KnowledgeSourceId {
    match target {
        DeletableKnowledgeId::Rumor(id) => KnowledgeSourceId::Rumor(id.clone()),
        DeletableKnowledgeId::Memory(id) => KnowledgeSourceId::Memory(id.clone()),
    }
}

pub fn modifiable_knowledge_index(ctx: &TurnExecutionContext) -> BTreeMap<KnowledgeSourceId, KnowledgeKind> {
    let mut index = BTreeMap::new();
    if let Some(baseline) = ctx.baseline() {
        for entry in &baseline.relevant_world_knowledge.facts {
            index.insert(entry.source_id.clone(), KnowledgeKind::Fact);
        }
        for entry in &baseline.relevant_world_knowledge.rumors {
            index.insert(entry.source_id.clone(), KnowledgeKind::Rumor);
        }
    }
    for item in &ctx.retrieved().world().facts {
        index.insert(item.source_id.clone(), KnowledgeKind::Fact);
    }
    for item in &ctx.retrieved().world().rumors {
        index.insert(item.source_id.clone(), KnowledgeKind::Rumor);
    }
    for character in ctx.retrieved().characters().values() {
        for item in &character.known_rumors {
            index.insert(item.source_id.clone(), KnowledgeKind::Rumor);
        }
        for item in &character.memories {
            index.insert(item.source_id.clone(), KnowledgeKind::Memory);
        }
    }
    index
}

fn has_duplicates<T: Ord + Clone>(values: &[T]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().any(|value| !seen.insert(value.clone()))
}

fn entity_resolves(entity: &KnowledgeEntity, snapshot: &StoryReadSnapshot) -> bool {
    match entity {
        KnowledgeEntity::Role(id) => snapshot.roles().contains_key(id),
        _ => snapshot.entity_catalog().contains(entity),
    }
}

fn location_key_resolves(key: &LocationKey, snapshot: &StoryReadSnapshot) -> bool {
    snapshot.entity_catalog().contains(&KnowledgeEntity::Location(key.clone()))
        || snapshot.roles().values().any(|role| &role.state.location == key)
}

fn issue(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ReferenceMissing,
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
#[path = "tests/reference_tests.rs"]
mod tests;
