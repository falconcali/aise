use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{LocationKey, SceneKey, TopicKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeIndexMatch, KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::planning::RetrievalAudience;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSignalOrigin {
    PlayerInput,
    Scene,
    Narrative,
    RecentStory,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntitySignal {
    pub entity: KnowledgeEntity,
    pub origin: RetrievalSignalOrigin,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopicSignal {
    pub topic: TopicKey,
    pub origin: RetrievalSignalOrigin,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalSignals {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub present_role_ids: Vec<RoleId>,
    pub entities: Vec<EntitySignal>,
    pub topics: Vec<TopicSignal>,
}

impl Default for RetrievalSignals {
    fn default() -> Self {
        Self {
            scene_key: SceneKey::from("unset"),
            location_key: LocationKey::from("unset"),
            present_role_ids: Vec::new(),
            entities: Vec::new(),
            topics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateRetrieverKind {
    Entity,
    Topic,
    Bm25,
    Embedding,
}

pub use crate::domain::knowledge::KnowledgeIndexMatch as CandidateMatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    Topic,
    Entity,
    EntityAndTopic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelevanceRank {
    pub match_level: MatchLevel,
    pub signal_priority: u8,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderEvidence {
    pub provider_rank: u32,
    pub matches: Vec<KnowledgeIndexMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextProvenance {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub source: KnowledgeSource,
    pub audience: RetrievalAudience,
    pub memory_owner: Option<RoleId>,
    pub evidence: BTreeMap<CandidateRetrieverKind, ProviderEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextItem {
    pub content: BoundedText,
    pub provenance: ContextProvenance,
    pub relevance: RelevanceRank,
    pub token_cost: u64,
}

impl ContextItem {
    pub fn from_parts(content: BoundedText, provenance: ContextProvenance, relevance: RelevanceRank) -> Self {
        let token_cost = estimate_text_tokens(content.as_str());
        Self {
            content,
            provenance,
            relevance,
            token_cost,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedContext {
    writer: Vec<ContextItem>,
    roles: BTreeMap<RoleId, Vec<ContextItem>>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetrievedContextLimits {
    pub max_role_audiences: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_item_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RetrievedContextError {
    #[error("retrieved context audience is invalid")]
    InvalidAudience,
    #[error("retrieved context memory owner is invalid")]
    InvalidMemoryOwner,
    #[error("retrieved context count limit exceeded: {limit}")]
    CountLimit { limit: &'static str },
    #[error("retrieved context item byte limit exceeded")]
    ItemByteLimit,
    #[error("retrieved context audience token limit exceeded")]
    AudienceTokenLimit,
    #[error("retrieved context total token limit exceeded")]
    TotalTokenLimit,
    #[error("retrieved context arithmetic overflow")]
    ArithmeticOverflow,
}

impl RetrievedContext {
    pub fn try_new(
        writer: Vec<ContextItem>,
        roles: BTreeMap<RoleId, Vec<ContextItem>>,
        limits: RetrievedContextLimits,
    ) -> Result<Self, RetrievedContextError> {
        if roles.len() > limits.max_role_audiences {
            return Err(RetrievedContextError::CountLimit {
                limit: "max_role_audiences",
            });
        }
        validate_partition(&writer, limits)?;
        for items in roles.values() {
            validate_partition(items, limits)?;
        }
        let role_items = roles.values().try_fold(0usize, |total, items| {
            total.checked_add(items.len()).ok_or(RetrievedContextError::ArithmeticOverflow)
        })?;
        let total_items = writer
            .len()
            .checked_add(role_items)
            .ok_or(RetrievedContextError::ArithmeticOverflow)?;
        if total_items > limits.max_total_items {
            return Err(RetrievedContextError::CountLimit {
                limit: "max_total_items",
            });
        }
        let mut total_tokens = partition_tokens(&writer)?;
        for items in roles.values() {
            total_tokens = total_tokens
                .checked_add(partition_tokens(items)?)
                .ok_or(RetrievedContextError::ArithmeticOverflow)?;
        }
        if total_tokens > limits.max_total_tokens {
            return Err(RetrievedContextError::TotalTokenLimit);
        }
        Ok(Self { writer, roles })
    }

    pub fn writer(&self) -> &[ContextItem] {
        &self.writer
    }

    pub fn for_role(&self, role_id: &RoleId) -> &[ContextItem] {
        self.roles.get(role_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn roles(&self) -> &BTreeMap<RoleId, Vec<ContextItem>> {
        &self.roles
    }

    pub fn total_items(&self) -> usize {
        self.writer.len().saturating_add(self.roles.values().map(Vec::len).sum())
    }

    pub fn total_tokens(&self) -> u64 {
        let mut total = 0u64;
        for item in &self.writer {
            total = total.saturating_add(item.token_cost);
        }
        for items in self.roles.values() {
            for item in items {
                total = total.saturating_add(item.token_cost);
            }
        }
        total
    }
}

fn validate_partition(items: &[ContextItem], limits: RetrievedContextLimits) -> Result<(), RetrievedContextError> {
    if items.len() > limits.max_items_per_audience {
        return Err(RetrievedContextError::CountLimit {
            limit: "max_items_per_audience",
        });
    }
    let mut tokens = 0u64;
    for item in items {
        if item.content.as_str().len() > limits.max_item_bytes {
            return Err(RetrievedContextError::ItemByteLimit);
        }
        if item.token_cost != estimate_text_tokens(item.content.as_str()) {
            return Err(RetrievedContextError::InvalidAudience);
        }
        match (
            &item.provenance.audience,
            item.provenance.knowledge_kind,
            &item.provenance.memory_owner,
        ) {
            (RetrievalAudience::GlobalWriter, KnowledgeKind::Memory, Some(_)) => {}
            (RetrievalAudience::Character { role_id }, KnowledgeKind::Memory, Some(owner)) if role_id == owner => {}
            (RetrievalAudience::Character { .. }, KnowledgeKind::Fact, _) => {
                return Err(RetrievedContextError::InvalidAudience);
            }
            (_, KnowledgeKind::Memory, _) => return Err(RetrievedContextError::InvalidMemoryOwner),
            (_, _, None) => {}
            (_, _, Some(_)) => return Err(RetrievedContextError::InvalidMemoryOwner),
        }
        tokens = tokens
            .checked_add(item.token_cost)
            .ok_or(RetrievedContextError::ArithmeticOverflow)?;
    }
    if tokens > limits.max_tokens_per_audience {
        return Err(RetrievedContextError::AudienceTokenLimit);
    }
    Ok(())
}

fn partition_tokens(items: &[ContextItem]) -> Result<u64, RetrievedContextError> {
    let mut tokens = 0u64;
    for item in items {
        tokens = tokens
            .checked_add(item.token_cost)
            .ok_or(RetrievedContextError::ArithmeticOverflow)?;
    }
    Ok(tokens)
}
