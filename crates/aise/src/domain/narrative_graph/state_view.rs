use crate::domain::asset::ids::FactKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::condition::RoleControllerKind;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NarrativeStateViewError {
    #[error("unknown role reference: {role_id}")]
    UnknownRole { role_id: String },
}

pub trait NarrativeStateView {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError>;

    fn role_attribute(
        &self,
        role_id: &RoleId,
        attribute: &BoundedText,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError>;

    fn relationship_trust(
        &self,
        source_role_id: &RoleId,
        target_role_id: &RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError>;

    fn role_controller(&self, role_id: &RoleId) -> Result<RoleControllerKind, NarrativeStateViewError>;
}

pub struct CommittedNarrativeStateView<'a> {
    snapshot: &'a StoryReadSnapshot,
}

impl<'a> CommittedNarrativeStateView<'a> {
    pub fn new(snapshot: &'a StoryReadSnapshot) -> Self {
        Self { snapshot }
    }
}

impl NarrativeStateView for CommittedNarrativeStateView<'_> {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        Ok(self.snapshot.fact_values().get(fact_key))
    }

    fn role_attribute(
        &self,
        role_id: &RoleId,
        attribute: &BoundedText,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        let role = self
            .snapshot
            .role(role_id)
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_id: role_id.as_str().to_owned(),
            })?;
        Ok(role
            .state
            .attributes
            .iter()
            .find(|(key, _)| key.as_str() == attribute.as_str())
            .map(|(_, value)| value))
    }

    fn relationship_trust(
        &self,
        source_role_id: &RoleId,
        target_role_id: &RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        if self.snapshot.role(source_role_id).is_none() {
            return Err(NarrativeStateViewError::UnknownRole {
                role_id: source_role_id.as_str().to_owned(),
            });
        }
        if self.snapshot.role(target_role_id).is_none() {
            return Err(NarrativeStateViewError::UnknownRole {
                role_id: target_role_id.as_str().to_owned(),
            });
        }
        Ok(self
            .snapshot
            .relationships()
            .iter()
            .find(|relationship| {
                &relationship.source_role_id == source_role_id && &relationship.target_role_id == target_role_id
            })
            .map(|relationship| relationship.trust))
    }

    fn role_controller(&self, role_id: &RoleId) -> Result<RoleControllerKind, NarrativeStateViewError> {
        let role = self
            .snapshot
            .role(role_id)
            .ok_or_else(|| NarrativeStateViewError::UnknownRole {
                role_id: role_id.as_str().to_owned(),
            })?;
        Ok(if role.is_player_controlled() {
            RoleControllerKind::Player
        } else {
            RoleControllerKind::Ai
        })
    }
}
