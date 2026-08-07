use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::CharacterId;
use crate::domain::knowledge::query::KnowledgeSourceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceScopeKind {
    Global,
    Character,
}

#[derive(Debug, Clone)]
pub struct AudienceScopedItem {
    pub scope: AudienceScopeKind,
    pub character_id: Option<CharacterId>,
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub source_id: KnowledgeSourceId,
    pub content: BoundedText,
    pub scope: AudienceScopedItem,
    pub relevance_score: f32,
    pub token_cost: u64,
}

impl ContextItem {
    pub fn for_global(source_id: KnowledgeSourceId, content: BoundedText, relevance_score: f32) -> Self {
        let token_cost = estimate_tokens(content.as_str());
        Self {
            source_id,
            content,
            scope: AudienceScopedItem {
                scope: AudienceScopeKind::Global,
                character_id: None,
            },
            relevance_score,
            token_cost,
        }
    }

    pub fn for_character(
        source_id: KnowledgeSourceId,
        character_id: CharacterId,
        content: BoundedText,
        relevance_score: f32,
    ) -> Self {
        let token_cost = estimate_tokens(content.as_str());
        Self {
            source_id,
            content,
            scope: AudienceScopedItem {
                scope: AudienceScopeKind::Character,
                character_id: Some(character_id),
            },
            relevance_score,
            token_cost,
        }
    }
}

pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).saturating_add(3) / 4
}
