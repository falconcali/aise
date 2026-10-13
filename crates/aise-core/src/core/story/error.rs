use crate::core::NarrativeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoryError {
    #[error("story pack defines no playable role")]
    NoPlayableRole,

    #[error("{field} references unknown role: {role_id}")]
    UnknownRole { field: &'static str, role_id: String },

    #[error("role is not playable: {role_id}")]
    RoleNotPlayable { role_id: String },

    #[error("narrative graph is invalid: {0}")]
    Narrative(#[from] NarrativeError),
}
