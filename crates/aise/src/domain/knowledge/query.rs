pub use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{PackId, Sha256Digest};
use crate::domain::error::KnowledgeIdError;
use crate::domain::ids::{FactId, MemoryId, RumorId, TurnId};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeIndexMatch {
    Entity(KnowledgeEntity),
    Topic(crate::domain::asset::ids::TopicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Fact,
    Rumor,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSource {
    Seed { pack_id: PackId, pack_digest: Sha256Digest },
    CommittedTurn { turn_id: TurnId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum KnowledgeSourceId {
    Fact(FactId),
    Rumor(RumorId),
    Memory(MemoryId),
}

impl KnowledgeSourceId {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Fact(id) => id.as_str(),
            Self::Rumor(id) => id.as_str(),
            Self::Memory(id) => id.as_str(),
        }
    }

    pub fn kind(&self) -> KnowledgeKind {
        match self {
            Self::Fact(_) => KnowledgeKind::Fact,
            Self::Rumor(_) => KnowledgeKind::Rumor,
            Self::Memory(_) => KnowledgeKind::Memory,
        }
    }

    pub fn try_from_parts(kind: KnowledgeKind, id: &str) -> Result<Self, KnowledgeIdError> {
        match kind {
            KnowledgeKind::Fact => FactId::try_new(id).map(Self::Fact),
            KnowledgeKind::Rumor => RumorId::try_new(id).map(Self::Rumor),
            KnowledgeKind::Memory => MemoryId::try_new(id).map(Self::Memory),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeSequence(NonZeroU64);

impl KnowledgeSequence {
    pub fn try_new(value: u64) -> Result<Self, KnowledgeIdError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| KnowledgeIdError::InvalidGrammar {
                value: value.to_string(),
            })
    }

    pub fn get(&self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct KnowledgeIdHighWater(u64);

impl KnowledgeIdHighWater {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeIdAllocation {
    pub assigned: Vec<KnowledgeSourceId>,
    pub new_high_water: KnowledgeIdHighWater,
}

pub fn new_knowledge_source_id(
    kind: KnowledgeKind,
    sequence: KnowledgeSequence,
) -> Result<KnowledgeSourceId, KnowledgeIdError> {
    let sequence = NonZeroU64::new(sequence.get()).expect("KnowledgeSequence invariant guarantees non-zero");
    match kind {
        KnowledgeKind::Fact => FactId::from_sequence(sequence).map(KnowledgeSourceId::Fact),
        KnowledgeKind::Rumor => RumorId::from_sequence(sequence).map(KnowledgeSourceId::Rumor),
        KnowledgeKind::Memory => MemoryId::from_sequence(sequence).map(KnowledgeSourceId::Memory),
    }
}

pub fn allocate_knowledge_ids(
    base: KnowledgeIdHighWater,
    addition_kinds: &[KnowledgeKind],
) -> Result<KnowledgeIdAllocation, KnowledgeIdError> {
    let mut next = base.get();
    let mut assigned = Vec::with_capacity(addition_kinds.len());
    for kind in addition_kinds {
        next = next.checked_add(1).ok_or(KnowledgeIdError::AllocationOverflow)?;
        let sequence = KnowledgeSequence::try_new(next)?;
        assigned.push(new_knowledge_source_id(*kind, sequence)?);
    }
    Ok(KnowledgeIdAllocation {
        assigned,
        new_high_water: KnowledgeIdHighWater::new(next),
    })
}

#[cfg(test)]
#[path = "tests/query_tests.rs"]
mod tests;
