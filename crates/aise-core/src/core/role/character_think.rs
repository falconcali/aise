use crate::core::{BoundedText, CharacterProfile, MemoryBody, MemoryId, RetrievalHint, RoleId};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkRequest {
    pub role_id: RoleId,
    pub thinking_focus: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedRole {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub aliases: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval_hint: RetrievalHint,
    pub profile: CharacterProfile,
    pub seed_memories: BTreeMap<MemoryId, MemoryBody>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterDecision {
    pub role_id: RoleId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
