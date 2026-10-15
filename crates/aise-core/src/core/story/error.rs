use crate::core::RoleId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoryError {
    #[error("story pack role count {actual} exceeds maximum {maximum}")]
    TooManyRoles { actual: usize, maximum: usize },

    #[error("role {role_id} alias count {actual} exceeds maximum {maximum}")]
    TooManyRoleAliases {
        role_id: RoleId,
        actual: usize,
        maximum: usize,
    },

    #[error("story pack role catalog size {actual} exceeds maximum {maximum}")]
    RoleCatalogTooLarge { actual: usize, maximum: usize },

    #[error("role {role_id} seed memory count {actual} exceeds maximum {maximum}")]
    TooManySeedMemories {
        role_id: RoleId,
        actual: usize,
        maximum: usize,
    },

    #[error("story pack role assets size {actual} exceeds maximum {maximum}")]
    RoleAssetsTooLarge { actual: usize, maximum: usize },

    #[error("story pack role assets cannot be serialized: {message}")]
    RoleAssetsSerialization { message: String },

    #[error("story pack defines no playable role")]
    NoPlayableRole,

    #[error("{field} references unknown role: {role_id}")]
    UnknownRole { field: &'static str, role_id: String },

    #[error("role is not playable: {role_id}")]
    RoleNotPlayable { role_id: String },
}
