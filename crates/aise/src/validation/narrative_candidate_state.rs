use crate::domain::asset::ids::{AttributeKey, FactKey, StoryRoleKey};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::CharacterId;
use crate::domain::narrative_graph::condition::RoleControllerKind;
use crate::domain::narrative_graph::state_view::{
    CommittedNarrativeStateView, NarrativeStateView, NarrativeStateViewError,
};
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::turn::turn_validation::{CharacterInstanceStateChange, RelationshipStateChange};

pub struct CandidateNarrativeStateView<'a> {
    committed: CommittedNarrativeStateView<'a>,
    snapshot: &'a StoryReadSnapshot,
    character_changes: &'a [CharacterInstanceStateChange],
    relationship_changes: &'a [RelationshipStateChange],
}

impl<'a> CandidateNarrativeStateView<'a> {
    pub fn new(
        snapshot: &'a StoryReadSnapshot,
        character_changes: &'a [CharacterInstanceStateChange],
        relationship_changes: &'a [RelationshipStateChange],
    ) -> Self {
        Self {
            committed: CommittedNarrativeStateView::new(snapshot),
            snapshot,
            character_changes,
            relationship_changes,
        }
    }
}

impl NarrativeStateView for CandidateNarrativeStateView<'_> {
    fn fact_value(&self, fact_key: &FactKey) -> Result<Option<&ScalarValue>, NarrativeStateViewError> {
        self.committed.fact_value(fact_key)
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
        if let Some(change) = self
            .character_changes
            .iter()
            .find(|change| change.character_id == binding.character_id)
        {
            return Ok(change.new_state.attributes.get(attribute));
        }
        self.committed.character_attribute(role_key, attribute)
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
        if let Some(change) = self.relationship_changes.iter().find(|change| {
            change.key.source_character_id == source.character_id
                && change.key.target_character_id == target.character_id
        }) {
            return Ok(Some(change.new_state.trust));
        }
        self.committed.relationship_trust(source_role_key, target_role_key)
    }

    fn role_controller(&self, role_key: &StoryRoleKey) -> Result<RoleControllerKind, NarrativeStateViewError> {
        self.committed.role_controller(role_key)
    }

    fn character_id_for_role(&self, role_key: &StoryRoleKey) -> Result<CharacterId, NarrativeStateViewError> {
        self.committed.character_id_for_role(role_key)
    }
}
