use crate::core::{BoundedText, RoleId};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkRequest {
    pub role_id: RoleId,
    pub thinking_focus: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterDecision {
    pub role_id: RoleId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
