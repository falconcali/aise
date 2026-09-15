use super::contracts::{ActivationEvidence, ActivationRejectionReason};
use crate::domain::asset::ids::Sha256Digest;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivationRejectionCounts {
    pub disabled: u32,
    pub scope_mismatch: u32,
    pub delayed: u32,
    pub cooldown: u32,
    pub recursion_excluded: u32,
    pub recursion_level_locked: u32,
    pub secondary_condition: u32,
    pub group_loser: u32,
    pub probability: u32,
    pub budget: u32,
    pub duplicate: u32,
    pub work_limit: u32,
}

impl ActivationRejectionCounts {
    pub fn from_summary(summary: &BTreeMap<ActivationRejectionReason, u32>) -> Self {
        let count = |reason| summary.get(&reason).copied().unwrap_or_default();
        Self {
            disabled: count(ActivationRejectionReason::Disabled),
            scope_mismatch: count(ActivationRejectionReason::ScopeMismatch),
            delayed: count(ActivationRejectionReason::Delayed),
            cooldown: count(ActivationRejectionReason::Cooldown),
            recursion_excluded: count(ActivationRejectionReason::RecursionExcluded),
            recursion_level_locked: count(ActivationRejectionReason::RecursionLevelLocked),
            secondary_condition: count(ActivationRejectionReason::SecondaryCondition),
            group_loser: count(ActivationRejectionReason::GroupLoser),
            probability: count(ActivationRejectionReason::Probability),
            budget: count(ActivationRejectionReason::Budget),
            duplicate: count(ActivationRejectionReason::Duplicate),
            work_limit: count(ActivationRejectionReason::WorkLimit),
        }
    }
}

pub fn bounded_evidence_digest(evidence: &[ActivationEvidence], max_bytes: usize) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"aise.activation.evidence.v1\x00");
    let mut consumed = 0usize;
    for item in evidence {
        let Ok(encoded) = serde_json::to_vec(item) else {
            continue;
        };
        let next = consumed.saturating_add(encoded.len());
        if next > max_bytes {
            break;
        }
        consumed = next;
        hasher.update((encoded.len() as u64).to_be_bytes());
        hasher.update(encoded);
    }
    hasher.update((consumed as u64).to_be_bytes());
    Sha256Digest::from_bytes(hasher.finalize().into())
}

#[cfg(test)]
#[path = "tests/evidence_tests.rs"]
mod tests;
