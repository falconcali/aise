use crate::core::{BoundedText, CharacterProfile, RoleId};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PlayerRoleContext {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub aliases: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub profile: CharacterProfile,
}
