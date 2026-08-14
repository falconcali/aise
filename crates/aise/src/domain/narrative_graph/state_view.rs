use crate::domain::asset::ids::{AttributeKey, FactKey, StoryRoleKey};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::CharacterId;
use crate::domain::narrative_graph::condition::RoleControllerKind;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NarrativeStateViewError {
    #[error("unknown role reference: {role_key}")]
    UnknownRole { role_key: String },
    #[error("unknown character reference for role: {role_key}")]
    UnknownCharacter { role_key: String },
}

pub trait NarrativeStateView {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError>;

    fn character_attribute(
        &self,
        role_key: &StoryRoleKey,
        attribute: &AttributeKey,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError>;

    fn relationship_trust(
        &self,
        source_role_key: &StoryRoleKey,
        target_role_key: &StoryRoleKey,
    ) -> Result<Option<i16>, NarrativeStateViewError>;

    fn role_controller(&self, role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError>;

    fn character_id_for_role(&self, role_key: &StoryRoleKey) -> Result<CharacterId, NarrativeStateViewError>;
}

pub struct CommittedNarrativeStateView<'a> {
    snapshot: &'a StoryReadSnapshot,
}

impl<'a> CommittedNarrativeStateView<'a> {
    pub fn new(snapshot: &'a StoryReadSnapshot) -> Self {
        Self { snapshot }
    }

    fn resolve_role_controller(&self, role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError> {
        let binding = self
            .snapshot
            .role_binding(role_key)
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_key: role_key.as_str().to_owned(),
            })?;
        Ok(if binding.is_player_controlled() {
            RoleControllerKind::Player
        } else {
            RoleControllerKind::Ai
        })
    }
}

impl NarrativeStateView for CommittedNarrativeStateView<'_> {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(self.snapshot.fact_values().get(fact_key))
    }

    fn character_attribute(
        &self,
        role_key: &StoryRoleKey,
        attribute: &AttributeKey,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        let binding = self
            .snapshot
            .role_binding(role_key)
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_key: role_key.as_str().to_owned(),
            })?;
        let character = self.snapshot.character_states().get(&binding.character_id).ok_or_else(|| {
            NarrativeStateViewError::UnknownCharacter {
                role_key: role_key.as_str().to_owned(),
            }
        })?;
        Ok(character.attributes.get(attribute))
    }

    fn relationship_trust(
        &self,
        source_role_key: &StoryRoleKey,
        target_role_key: &StoryRoleKey,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        let source =
            self.snapshot
                .role_binding(source_role_key)
                .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                    role_key: source_role_key.as_str().to_owned(),
                })?;
        let target =
            self.snapshot
                .role_binding(target_role_key)
                .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                    role_key: target_role_key.as_str().to_owned(),
                })?;
        Ok(self
            .snapshot
            .relationships()
            .iter()
            .find(|relationship| {
                relationship.source_character_id == source.character_id
                    && relationship.target_character_id == target.character_id
            })
            .map(|relationship| relationship.trust))
    }

    fn role_controller(&self, role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError> {
        self.resolve_role_controller(role_key)
    }

    fn character_id_for_role(&self, role_key: &StoryRoleKey) -> Result<CharacterId, NarrativeStateViewError> {
        let binding = self
            .snapshot
            .role_binding(role_key)
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_key: role_key.as_str().to_owned(),
            })?;
        Ok(binding.character_id.clone())
    }
}
