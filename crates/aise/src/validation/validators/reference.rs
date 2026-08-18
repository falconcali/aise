use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::LocationKey;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
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
        let dto = extraction;
        let established_roles = snapshot.roles().keys().cloned().collect::<BTreeSet<_>>();
        let mut known_roles = established_roles.clone();
        for role in &dto.new_roles {
            if let Ok(role_id) = RoleId::try_new(role.role_id.clone()) {
                known_roles.insert(role_id);
            }
        }
        let modifiable = modifiable_knowledge_index(ctx);
        let mut issues = Vec::new();

        for (index, role) in dto.new_roles.iter().enumerate() {
            if let Ok(role_id) = RoleId::try_new(role.role_id.clone()) {
                if established_roles.contains(&role_id) {
                    issues.push(issue("new_roles", index, "role_id already exists in the story"));
                }
            } else {
                issues.push(issue("new_roles", index, "role_id does not resolve to a valid identifier"));
            }
            if !location_key_resolves_str(&role.location, snapshot) {
                issues.push(issue("new_roles", index, "location does not resolve to a known location"));
            }
        }

        for (index, state) in dto.role_states.iter().enumerate() {
            let role_id = RoleId::try_new(state.role_id.clone()).ok();
            if !role_id.is_some_and(|role_id| known_roles.contains(&role_id)) {
                issues.push(issue("role_states", index, "role_id is not a known role"));
            }
            if !location_key_resolves_str(&state.location, snapshot) {
                issues.push(issue("role_states", index, "location does not resolve to a known location"));
            }
        }

        for (index, relationship) in dto.relationship_states.iter().enumerate() {
            let source = RoleId::try_new(relationship.source_role_id.clone()).ok();
            let target = RoleId::try_new(relationship.target_role_id.clone()).ok();
            let resolves = source.is_some_and(|role_id| known_roles.contains(&role_id))
                && target.is_some_and(|role_id| known_roles.contains(&role_id));
            if !resolves {
                issues.push(issue("relationship_states", index, "relationship references an unknown role"));
            }
        }

        for (index, draft) in dto.add_rumors.iter().enumerate() {
            if !source_role_resolves(&draft.source_role_id, &known_roles) {
                issues.push(issue("add_rumors", index, "source_role_id is not a known role"));
            }
        }
        for (index, update) in dto.update_rumors.iter().enumerate() {
            if !source_role_resolves(&update.source_role_id, &known_roles) {
                issues.push(issue("update_rumors", index, "source_role_id is not a known role"));
            }
            if !modifiable_contains(&modifiable, KnowledgeKind::Rumor, &update.id) {
                issues.push(issue("update_rumors", index, "update target is not modifiable"));
            }
        }
        for (index, draft) in dto.add_memories.iter().enumerate() {
            let owner = RoleId::try_new(draft.owner_role_id.clone()).ok();
            if !owner.is_some_and(|role_id| known_roles.contains(&role_id)) {
                issues.push(issue("add_memories", index, "owner_role_id is not a known role"));
            }
        }
        for (index, update) in dto.update_memories.iter().enumerate() {
            if !modifiable_contains(&modifiable, KnowledgeKind::Memory, &update.id) {
                issues.push(issue("update_memories", index, "update target is not modifiable"));
            }
        }
        for (index, update) in dto.update_facts.iter().enumerate() {
            if !modifiable_contains(&modifiable, KnowledgeKind::Fact, &update.id) {
                issues.push(issue("update_facts", index, "update target is not modifiable"));
            }
        }
        for (index, raw_id) in dto.delete_rumor_ids.iter().enumerate() {
            if !modifiable_contains(&modifiable, KnowledgeKind::Rumor, raw_id) {
                issues.push(issue("delete_rumor_ids", index, "delete target is not modifiable"));
            }
        }
        for (index, raw_id) in dto.delete_memory_ids.iter().enumerate() {
            if !modifiable_contains(&modifiable, KnowledgeKind::Memory, raw_id) {
                issues.push(issue("delete_memory_ids", index, "delete target is not modifiable"));
            }
        }

        Ok(issues)
    }
}

fn source_role_resolves(raw: &str, known_roles: &BTreeSet<RoleId>) -> bool {
    if raw.trim().is_empty() {
        return true;
    }
    RoleId::try_new(raw).is_ok_and(|role_id| known_roles.contains(&role_id))
}

fn modifiable_contains(
    modifiable: &BTreeMap<KnowledgeSourceId, KnowledgeKind>,
    kind: KnowledgeKind,
    raw_id: &str,
) -> bool {
    let source_id = match kind {
        KnowledgeKind::Fact => crate::domain::ids::FactId::try_new(raw_id.to_owned())
            .ok()
            .map(KnowledgeSourceId::Fact),
        KnowledgeKind::Rumor => crate::domain::ids::RumorId::try_new(raw_id.to_owned())
            .ok()
            .map(KnowledgeSourceId::Rumor),
        KnowledgeKind::Memory => crate::domain::ids::MemoryId::try_new(raw_id.to_owned())
            .ok()
            .map(KnowledgeSourceId::Memory),
    };
    source_id.is_some_and(|source_id| modifiable.get(&source_id) == Some(&kind))
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

fn location_key_resolves_str(raw: &str, snapshot: &StoryReadSnapshot) -> bool {
    let Ok(key) = LocationKey::try_new(raw) else {
        return false;
    };
    snapshot.entity_catalog().contains(&KnowledgeEntity::Location(key.clone()))
        || snapshot.roles().values().any(|role| role.state.location == key)
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
