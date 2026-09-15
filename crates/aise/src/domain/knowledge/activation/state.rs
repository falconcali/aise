use super::rule::ActivationRuleVersion;
use crate::domain::ids::TurnNumber;
use crate::domain::knowledge::query::KnowledgeSourceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationRunMode {
    CommitEligible,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationSeedKind {
    Constant,
    Sticky,
    TextMatch,
    PlannerExactTarget,
    Provider,
    PreviewOverride,
    Recursion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationStopReason {
    Complete,
    MinimumSatisfied,
    MaximumDepthReached,
    RecursionExhausted,
    WorkTrimmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationTimedState {
    pub source_id: KnowledgeSourceId,
    pub rule_version: ActivationRuleVersion,
    pub sticky_through_turn: Option<TurnNumber>,
    pub cooldown_through_turn: Option<TurnNumber>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingActivationStateDelta {
    pub upserts: Vec<ActivationTimedState>,
    pub deletes: BTreeSet<KnowledgeSourceId>,
}

impl PendingActivationStateDelta {
    pub fn merge(&mut self, newer: Self) {
        let mut upserts = self
            .upserts
            .drain(..)
            .map(|state| (state.source_id.clone(), state))
            .collect::<std::collections::BTreeMap<_, _>>();
        for source_id in newer.deletes {
            upserts.remove(&source_id);
            self.deletes.insert(source_id);
        }
        for state in newer.upserts {
            self.deletes.remove(&state.source_id);
            upserts.insert(state.source_id.clone(), state);
        }
        self.upserts = upserts.into_values().collect();
    }
}
