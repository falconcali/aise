use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{LocationKey, SceneKey};
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
        let known_characters = snapshot.character_states().keys().cloned().collect::<BTreeSet<_>>();
        let modifiable = modifiable_knowledge_index(ctx);
        let mut issues = Vec::new();

        for (index, state) in extraction.character_states.iter().enumerate() {
            if !known_characters.contains(&state.character_id) {
                issues.push(issue("character_states", index, "character_id is not a known character"));
            }
            if !location_key_resolves(
                &state.location,
                &snapshot.current_scene().location_key,
                snapshot.entity_catalog(),
            ) {
                issues.push(issue(
                    "character_states",
                    index,
                    "location does not resolve to a known location",
                ));
            }
        }

        for (index, relationship) in extraction.relationship_states.iter().enumerate() {
            if !known_characters.contains(&relationship.source_character_id)
                || !known_characters.contains(&relationship.target_character_id)
            {
                issues.push(issue(
                    "relationship_states",
                    index,
                    "relationship references an unknown character",
                ));
            }
        }

        for (index, mutation) in extraction.knowledge_changes.iter().enumerate() {
            match mutation {
                ProposedKnowledgeMutation::Add { value } => {
                    if let Some(message) = invalid_value_reference(value, &known_characters, snapshot) {
                        issues.push(issue("knowledge_changes", index, message));
                    }
                }
                ProposedKnowledgeMutation::Update { target, value } => {
                    match modifiable.get(target) {
                        Some(kind) if *kind == value_kind(value) => {}
                        Some(_) => issues.push(issue("knowledge_changes", index, "update target kind mismatch")),
                        None => issues.push(issue("knowledge_changes", index, "update target is not modifiable")),
                    }
                    if let Some(message) = invalid_value_reference(value, &known_characters, snapshot) {
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

        let scene = &extraction.current_scene;
        if !scene_key_resolves(&scene.scene_key, &snapshot.current_scene().scene_key, snapshot.entity_catalog()) {
            issues.push(issue("current_scene", 0, "scene_key does not resolve to a known scene"));
        }
        if !location_key_resolves(
            &scene.location_key,
            &snapshot.current_scene().location_key,
            snapshot.entity_catalog(),
        ) {
            issues.push(issue("current_scene", 0, "location_key does not resolve to a known location"));
        }
        let mut seen = BTreeSet::new();
        for character_id in &scene.present_character_ids {
            if !known_characters.contains(character_id) || !seen.insert(character_id.clone()) {
                issues.push(issue(
                    "current_scene",
                    0,
                    "present_character_ids contains an unknown or duplicate id",
                ));
                break;
            }
        }

        Ok(issues)
    }
}

fn invalid_value_reference(
    value: &ProposedKnowledgeValue,
    known_characters: &BTreeSet<crate::domain::ids::CharacterId>,
    snapshot: &StoryReadSnapshot,
) -> Option<&'static str> {
    let (entities, topics, memory_owner, rumor_source) = match value {
        ProposedKnowledgeValue::Fact { entities, topics, .. } => (entities, topics, None, None),
        ProposedKnowledgeValue::Rumor {
            entities,
            topics,
            source_character_id,
            ..
        } => (entities, topics, None, source_character_id.as_ref()),
        ProposedKnowledgeValue::Memory {
            entities,
            topics,
            owner,
            ..
        } => (entities, topics, Some(owner), None),
    };
    if memory_owner.is_some_and(|owner| !known_characters.contains(owner)) {
        return Some("memory owner is not a known character");
    }
    if rumor_source.is_some_and(|character| !known_characters.contains(character)) {
        return Some("rumor source_character_id is not a known character");
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
        for entry in &baseline.relevant_knowledge {
            index.insert(entry.entry_id.clone(), entry.kind);
        }
    }
    for item in ctx.retrieved().writer() {
        index.insert(item.provenance.source_id.clone(), item.provenance.knowledge_kind);
    }
    for items in ctx.retrieved().characters().values() {
        for item in items {
            index.insert(item.provenance.source_id.clone(), item.provenance.knowledge_kind);
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
        KnowledgeEntity::Role(key) => snapshot.role_bindings().contains_key(key),
        KnowledgeEntity::Character(id) => snapshot.character_states().contains_key(id),
        KnowledgeEntity::Scene(key) => {
            scene_key_resolves(key, &snapshot.current_scene().scene_key, snapshot.entity_catalog())
        }
        KnowledgeEntity::Location(key) => {
            location_key_resolves(key, &snapshot.current_scene().location_key, snapshot.entity_catalog())
        }
        _ => snapshot.entity_catalog().contains(entity),
    }
}

fn scene_key_resolves(key: &SceneKey, current: &SceneKey, catalog: &[KnowledgeEntity]) -> bool {
    key == current || catalog.contains(&KnowledgeEntity::Scene(key.clone()))
}

fn location_key_resolves(key: &LocationKey, current: &LocationKey, catalog: &[KnowledgeEntity]) -> bool {
    key == current || catalog.contains(&KnowledgeEntity::Location(key.clone()))
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
