use crate::domain::asset::ids::FactKey;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::RoleId;
use crate::domain::narrative_graph::condition::RoleControllerKind;
use crate::domain::narrative_graph::state_view::{
    CommittedNarrativeStateView, NarrativeStateView, NarrativeStateViewError,
};
use crate::domain::story_instance::role::StoryRole;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::turn::turn_validation::{RoleStateChange, ValidatedRelationshipOperation};

pub struct CandidateNarrativeStateView<'a> {
    committed: CommittedNarrativeStateView<'a>,
    new_roles: &'a [StoryRole],
    role_changes: &'a [RoleStateChange],
    relationship_operations: &'a [ValidatedRelationshipOperation],
}

impl<'a> CandidateNarrativeStateView<'a> {
    pub fn new(
        snapshot: &'a StoryReadSnapshot,
        new_roles: &'a [StoryRole],
        role_changes: &'a [RoleStateChange],
        relationship_operations: &'a [ValidatedRelationshipOperation],
    ) -> Self {
        Self {
            committed: CommittedNarrativeStateView::new(snapshot),
            new_roles,
            role_changes,
            relationship_operations,
        }
    }
}

impl NarrativeStateView for CandidateNarrativeStateView<'_> {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        self.committed.fact_value(fact_key)
    }

    fn role_attribute(
        &self,
        role_id: &RoleId,
        attribute: &BoundedText,
    ) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        if let Some(change) = self.role_changes.iter().find(|change| &change.role_id == role_id) {
            return Ok(change
                .new_state
                .attributes
                .iter()
                .find(|(key, _)| key.as_str() == attribute.as_str())
                .map(|(_, value)| value));
        }
        if let Some(role) = self.new_roles.iter().find(|role| &role.role_id == role_id) {
            return Ok(role
                .state
                .attributes
                .iter()
                .find(|(key, _)| key.as_str() == attribute.as_str())
                .map(|(_, value)| value));
        }
        self.committed.role_attribute(role_id, attribute)
    }

    fn relationship_trust(
        &self,
        source_role_id: &RoleId,
        target_role_id: &RoleId,
    ) -> Result<Option<i16>, NarrativeStateViewError> {
        if let Some(operation) = self.relationship_operations.iter().find(|operation| {
            let key = operation.new_state().key();
            &key.source_role_id == source_role_id && &key.target_role_id == target_role_id
        }) {
            return Ok(Some(operation.new_state().trust));
        }
        self.committed.relationship_trust(source_role_id, target_role_id)
    }

    fn role_controller(&self, role_id: &RoleId) -> Result<RoleControllerKind, NarrativeStateViewError> {
        if self.new_roles.iter().any(|role| &role.role_id == role_id) {
            return Ok(RoleControllerKind::Ai);
        }
        self.committed.role_controller(role_id)
    }
}
