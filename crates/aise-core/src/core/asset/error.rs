use crate::core::NarrativeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("asset text too long: {field} (actual: {actual}, maximum: {maximum})")]
    TextTooLong {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },

    #[error("asset has too many items: {field} (actual: {actual}, maximum: {maximum})")]
    TooManyItems {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },

    #[error("story pack defines no playable role")]
    NoPlayableRole,

    #[error("{field} references unknown role: {role_id}")]
    UnknownRole { field: &'static str, role_id: String },

    #[error("role is not playable: {role_id}")]
    RoleNotPlayable { role_id: String },

    #[error("narrative graph is invalid: {0}")]
    Narrative(#[from] NarrativeError),

    #[error("{section} entry {id} is invalid: {source}")]
    InvalidEntry {
        section: &'static str,
        id: String,
        #[source]
        source: Box<AssetError>,
    },
}
