use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::activation::{ActivationRuleVersion, ActivationSeedKind, KnowledgeActivationRule};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSource, KnowledgeSourceId};
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::baseline::RoleContextView;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct RetrievedKnowledgeItem {
    pub source_id: KnowledgeSourceId,
    pub content: BoundedText,
    pub source: KnowledgeSource,
    pub activation_class: ActivationSeedKind,
    pub rank: u32,
    pub salience: u8,
    pub token_cost: u64,
    #[serde(skip)]
    pub activation: Option<KnowledgeActivationRule>,
    #[serde(skip)]
    pub activation_rule_version: Option<ActivationRuleVersion>,
}

impl RetrievedKnowledgeItem {
    pub fn from_parts(
        source_id: KnowledgeSourceId,
        content: BoundedText,
        source: KnowledgeSource,
        activation_class: ActivationSeedKind,
        rank: u32,
        salience: u8,
    ) -> Self {
        let token_cost = estimate_text_tokens(content.as_str());
        Self {
            source_id,
            content,
            source,
            activation_class,
            rank,
            salience,
            token_cost,
            activation: None,
            activation_rule_version: None,
        }
    }

    pub fn with_activation(
        mut self,
        activation: KnowledgeActivationRule,
        activation_rule_version: ActivationRuleVersion,
    ) -> Self {
        self.activation = Some(activation);
        self.activation_rule_version = Some(activation_rule_version);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedWorldKnowledge {
    pub facts: Vec<RetrievedKnowledgeItem>,
    pub rumors: Vec<RetrievedKnowledgeItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedCharacterContext {
    pub role: Option<RoleContextView>,
    pub known_rumors: Vec<RetrievedKnowledgeItem>,
    pub memories: Vec<RetrievedKnowledgeItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedContext {
    world: RetrievedWorldKnowledge,
    characters: BTreeMap<RoleId, RetrievedCharacterContext>,
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
    #[error("retrieved context kind is invalid for its partition")]
    InvalidKind,
    #[error("retrieved context memory owner is invalid")]
    InvalidMemoryOwner,
    #[error("retrieved context role is invalid")]
    InvalidRole,
    #[error("retrieved context contains a conflicting duplicate")]
    ConflictingDuplicate,
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

impl RetrievedContextError {
    pub fn turn_code(&self) -> &'static str {
        match self {
            RetrievedContextError::InvalidKind
            | RetrievedContextError::InvalidMemoryOwner
            | RetrievedContextError::InvalidRole
            | RetrievedContextError::ConflictingDuplicate => "retrieval_partition_invalid",
            RetrievedContextError::CountLimit { .. }
            | RetrievedContextError::ItemByteLimit
            | RetrievedContextError::AudienceTokenLimit
            | RetrievedContextError::TotalTokenLimit
            | RetrievedContextError::ArithmeticOverflow => "retrieval_context_limit",
        }
    }
}

impl RetrievedContext {
    pub fn try_new(
        world: RetrievedWorldKnowledge,
        characters: BTreeMap<RoleId, RetrievedCharacterContext>,
        limits: RetrievedContextLimits,
    ) -> Result<Self, RetrievedContextError> {
        if characters.len() > limits.max_role_audiences {
            return Err(RetrievedContextError::CountLimit {
                limit: "max_role_audiences",
            });
        }
        validate_kind_only(&world.facts, KnowledgeKind::Fact)?;
        validate_kind_only(&world.rumors, KnowledgeKind::Rumor)?;
        validate_partition_bounds(&world.facts, limits)?;
        validate_partition_bounds(&world.rumors, limits)?;
        for (role_id, character) in &characters {
            if let Some(role) = &character.role {
                if role.role_id != *role_id {
                    return Err(RetrievedContextError::InvalidRole);
                }
            }
            validate_kind_only(&character.known_rumors, KnowledgeKind::Rumor)?;
            validate_kind_only(&character.memories, KnowledgeKind::Memory)?;
            validate_partition_bounds(&character.known_rumors, limits)?;
            validate_partition_bounds(&character.memories, limits)?;
        }
        let character_items = characters.values().try_fold(0usize, |total, character| {
            total
                .checked_add(character.known_rumors.len())
                .and_then(|value| value.checked_add(character.memories.len()))
                .ok_or(RetrievedContextError::ArithmeticOverflow)
        })?;
        let total_items = world
            .facts
            .len()
            .checked_add(world.rumors.len())
            .and_then(|value| value.checked_add(character_items))
            .ok_or(RetrievedContextError::ArithmeticOverflow)?;
        if total_items > limits.max_total_items {
            return Err(RetrievedContextError::CountLimit {
                limit: "max_total_items",
            });
        }
        let mut total_tokens = checked_add_tokens(0, &world.facts)?;
        total_tokens = checked_add_tokens(total_tokens, &world.rumors)?;
        for character in characters.values() {
            total_tokens = checked_add_tokens(total_tokens, &character.known_rumors)?;
            total_tokens = checked_add_tokens(total_tokens, &character.memories)?;
        }
        if total_tokens > limits.max_total_tokens {
            return Err(RetrievedContextError::TotalTokenLimit);
        }
        Ok(Self { world, characters })
    }

    pub fn world(&self) -> &RetrievedWorldKnowledge {
        &self.world
    }

    pub fn character(&self, role_id: &RoleId) -> Option<&RetrievedCharacterContext> {
        self.characters.get(role_id)
    }

    pub fn characters(&self) -> &BTreeMap<RoleId, RetrievedCharacterContext> {
        &self.characters
    }

    pub fn total_items(&self) -> usize {
        let character_items: usize = self
            .characters
            .values()
            .map(|character| character.known_rumors.len().saturating_add(character.memories.len()))
            .sum();
        self.world
            .facts
            .len()
            .saturating_add(self.world.rumors.len())
            .saturating_add(character_items)
    }

    pub fn total_tokens(&self) -> u64 {
        let mut total = sum_tokens(&self.world.facts).saturating_add(sum_tokens(&self.world.rumors));
        for character in self.characters.values() {
            total = total
                .saturating_add(sum_tokens(&character.known_rumors))
                .saturating_add(sum_tokens(&character.memories));
        }
        total
    }
}

fn validate_kind_only(items: &[RetrievedKnowledgeItem], kind: KnowledgeKind) -> Result<(), RetrievedContextError> {
    let mut seen = std::collections::BTreeSet::new();
    for item in items {
        if item.source_id.kind() != kind {
            return Err(RetrievedContextError::InvalidKind);
        }
        if !seen.insert(item.source_id.clone()) {
            return Err(RetrievedContextError::ConflictingDuplicate);
        }
    }
    Ok(())
}

fn validate_partition_bounds(
    items: &[RetrievedKnowledgeItem],
    limits: RetrievedContextLimits,
) -> Result<(), RetrievedContextError> {
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
            return Err(RetrievedContextError::InvalidKind);
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

fn checked_add_tokens(base: u64, items: &[RetrievedKnowledgeItem]) -> Result<u64, RetrievedContextError> {
    let mut total = base;
    for item in items {
        total = total
            .checked_add(item.token_cost)
            .ok_or(RetrievedContextError::ArithmeticOverflow)?;
    }
    Ok(total)
}

fn sum_tokens(items: &[RetrievedKnowledgeItem]) -> u64 {
    items.iter().fold(0u64, |total, item| total.saturating_add(item.token_cost))
}

#[cfg(test)]
#[path = "tests/retrieval_tests.rs"]
mod tests;
