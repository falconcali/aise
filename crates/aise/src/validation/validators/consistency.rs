use crate::core::story_proposal::ProposedKnowledgeChange;
use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_validation::{Repairability, ValidationIssue, ValidationIssueCode, ValidationLocation};
use crate::domain::asset::entity::KnowledgeEntity;
use crate::validation::validators::DeterministicValidator;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct ConsistencyValidator;

impl DeterministicValidator for ConsistencyValidator {
    fn code(&self) -> ValidationIssueCode {
        ValidationIssueCode::ReferenceMissing
    }

    fn validate(&self, ctx: &TurnExecutionContext) -> Result<Vec<ValidationIssue>, TurnExecutionError> {
        let Some(proposal) = ctx.proposal() else {
            return Ok(Vec::new());
        };
        let Some(snapshot) = ctx.snapshot() else {
            return Ok(Vec::new());
        };
        let mut issues = Vec::new();
        let known = snapshot.character_states().keys().cloned().collect::<BTreeSet<_>>();
        let mut changed_characters = BTreeSet::new();
        for (index, change) in proposal.character_changes.iter().enumerate() {
            if !known.contains(&change.character_id) || !changed_characters.insert(change.character_id.clone()) {
                issues.push(issue("character_changes", index, "unknown or duplicate character change"));
            }
        }
        let relationships = snapshot
            .relationships()
            .iter()
            .map(|relationship| relationship.key())
            .collect::<BTreeSet<_>>();
        let mut changed_relationships = BTreeSet::new();
        for (index, change) in proposal.relationship_changes.iter().enumerate() {
            let key = crate::domain::story_instance::state::RelationshipKey {
                source_character_id: change.source_character_id.clone(),
                target_character_id: change.target_character_id.clone(),
                kind: change.kind.clone(),
            };
            if !known.contains(&change.source_character_id)
                || !known.contains(&change.target_character_id)
                || !relationships.contains(&key)
                || !changed_relationships.insert(key)
            {
                issues.push(issue("relationship_changes", index, "unknown or duplicate relationship change"));
            }
        }
        for (index, change) in proposal.knowledge_changes.iter().enumerate() {
            let (owner, entities, topics, source_character, source_event) = match change {
                ProposedKnowledgeChange::Fact {
                    entities,
                    topics,
                    evidence,
                    ..
                } => {
                    let invalid_event = evidence.iter().any(|evidence| match evidence {
                        crate::core::story_proposal::WorldFactEvidenceRef::ProposedEvent { event_index } => {
                            usize::try_from(*event_index).map_or(true, |value| value >= proposal.events.len())
                        }
                        _ => false,
                    });
                    (None, entities, topics, None, invalid_event)
                }
                ProposedKnowledgeChange::Rumor {
                    entities,
                    topics,
                    source_character_id,
                    source_event_index,
                    ..
                } => (
                    None,
                    entities,
                    topics,
                    source_character_id.as_ref(),
                    invalid_event(*source_event_index, proposal.events.len()),
                ),
                ProposedKnowledgeChange::Memory {
                    owner,
                    entities,
                    topics,
                    source_event_index,
                    ..
                } => (
                    Some(owner),
                    entities,
                    topics,
                    None,
                    invalid_event(*source_event_index, proposal.events.len()),
                ),
            };
            if owner.is_some_and(|owner| !known.contains(owner))
                || source_character.is_some_and(|character| !known.contains(character))
                || source_event
                || entities.iter().any(|entity| !entity_resolves(entity, snapshot))
                || topics.iter().any(|topic| !snapshot.topic_dictionary().contains_key(topic))
                || has_duplicates(entities)
                || has_duplicates(topics)
                || owner.is_some_and(|owner| {
                    entities
                        .iter()
                        .any(|entity| matches!(entity, KnowledgeEntity::Character(character) if character != owner))
                })
            {
                issues.push(issue(
                    "knowledge_changes",
                    index,
                    "knowledge change contains an invalid reference",
                ));
            }
        }
        for (index, perception) in proposal.perceptions.iter().enumerate() {
            if !known.contains(&perception.character_id)
                || usize::try_from(perception.source_event_index).map_or(true, |value| value >= proposal.events.len())
            {
                issues.push(issue("perceptions", index, "perception contains an invalid reference"));
            }
        }
        if let Some(scene) = &proposal.scene_change {
            if scene.present_character_ids.iter().any(|character| !known.contains(character))
                || has_duplicates(&scene.present_character_ids)
                || !snapshot
                    .entity_catalog()
                    .contains(&KnowledgeEntity::Scene(scene.scene_key.clone()))
                || !snapshot
                    .entity_catalog()
                    .contains(&KnowledgeEntity::Location(scene.location_key.clone()))
            {
                issues.push(issue("scene_change", 0, "scene contains an invalid reference"));
            }
        }
        Ok(issues)
    }
}

fn has_duplicates<T: Ord + Clone>(values: &[T]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().any(|value| !seen.insert(value.clone()))
}

fn invalid_event(index: Option<u32>, count: usize) -> bool {
    index.is_some_and(|index| usize::try_from(index).map_or(true, |value| value >= count))
}

fn entity_resolves(
    entity: &KnowledgeEntity,
    snapshot: &crate::domain::story_instance::snapshot::StoryReadSnapshot,
) -> bool {
    match entity {
        KnowledgeEntity::Role(key) => snapshot.role_bindings().contains_key(key),
        KnowledgeEntity::Character(id) => snapshot.character_states().contains_key(id),
        _ => snapshot.entity_catalog().contains(entity),
    }
}

fn issue(path: &str, index: usize, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: ValidationIssueCode::ReferenceMissing,
        message: message.into(),
        repairability: Repairability::Fatal,
        location: Some(ValidationLocation {
            path: format!("{path}[{index}]"),
            item_index: u32::try_from(index).ok(),
        }),
    }
}
