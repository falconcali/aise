use super::rule::{ActivationGroupKey, ActivationRuleVersion, GenerationTrigger, KnowledgeActivationRule};
use super::scan::{ActivationScanBuffer, ScanFragmentId, ScanFragmentKind};
use super::state::{
    ActivationRunMode, ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta,
};
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{StoryId, StoryRevision, TurnNumber};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::KnowledgeDelivery;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationIndexSnapshotRef {
    pub story_id: StoryId,
    pub pack_digest: Sha256Digest,
    pub base_revision: StoryRevision,
    pub overlay_version: u64,
    pub matcher_version: u32,
}

impl ActivationIndexSnapshotRef {
    pub fn from_knowledge(snapshot: &KnowledgeSnapshotRef, overlay_version: u64, matcher_version: u32) -> Self {
        Self {
            story_id: snapshot.story_id.clone(),
            pack_digest: snapshot.pack_digest.clone(),
            base_revision: snapshot.base_revision,
            overlay_version,
            matcher_version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivationEntryMetadata {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub rule: KnowledgeActivationRule,
    pub rule_version: ActivationRuleVersion,
    pub salience: u8,
}

#[derive(Debug, Clone)]
pub struct ActivationIndexSnapshot {
    pub reference: ActivationIndexSnapshotRef,
    pub constant_entries: Vec<KnowledgeSourceId>,
    pub metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
    execution_input: Option<ActivationExecutionInput>,
}

impl ActivationIndexSnapshot {
    pub fn new(
        reference: ActivationIndexSnapshotRef,
        metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
    ) -> Self {
        let constant_entries = metadata
            .values()
            .filter(|entry| entry.rule.mode.constant)
            .map(|entry| entry.source_id.clone())
            .collect();
        Self {
            reference,
            constant_entries,
            metadata,
            execution_input: None,
        }
    }

    pub fn with_execution_input(mut self, input: ActivationExecutionInput) -> Self {
        self.execution_input = Some(input);
        self
    }

    pub fn execution_input(&self) -> Option<&ActivationExecutionInput> {
        self.execution_input.as_ref()
    }

    pub fn matches_snapshot(&self, snapshot: &KnowledgeSnapshotRef, matcher_version: u32) -> bool {
        self.reference.story_id == snapshot.story_id
            && self.reference.pack_digest == snapshot.pack_digest
            && self.reference.base_revision == snapshot.base_revision
            && self.reference.matcher_version == matcher_version
    }
}

#[derive(Debug, Clone)]
pub struct ActivationEntryInput {
    pub source_id: KnowledgeSourceId,
    pub deliveries: Vec<KnowledgeDelivery>,
    pub token_cost: u64,
    pub body: BoundedText,
}

#[derive(Debug, Clone)]
pub struct ActivationMacroValues {
    pub player_name: String,
    pub player_role_label: String,
}

#[derive(Debug, Clone)]
pub struct ActivationExecutionInput {
    entries: BTreeMap<KnowledgeSourceId, ActivationEntryInput>,
    macros: ActivationMacroValues,
    max_macro_expansion_bytes: usize,
}

impl ActivationExecutionInput {
    pub fn try_new(
        entries: Vec<ActivationEntryInput>,
        macros: ActivationMacroValues,
        max_entries: usize,
        max_total_body_bytes: usize,
        max_macro_value_bytes: usize,
        max_macro_expansion_bytes: usize,
    ) -> Result<Self, ActivationInputError> {
        if max_entries == 0 || max_total_body_bytes == 0 || max_macro_value_bytes == 0 || max_macro_expansion_bytes == 0
        {
            return Err(ActivationInputError::InvalidLimit);
        }
        if entries.len() > max_entries {
            return Err(ActivationInputError::EntryLimit);
        }
        if macros.player_name.len() > max_macro_value_bytes
            || macros.player_role_label.len() > max_macro_value_bytes
            || macros
                .player_name
                .len()
                .checked_add(macros.player_role_label.len())
                .is_none_or(|value| value > max_macro_expansion_bytes)
        {
            return Err(ActivationInputError::MacroLimit);
        }
        let mut mapped = BTreeMap::new();
        let mut total_body_bytes = 0usize;
        for mut entry in entries {
            if entry.token_cost == 0 || entry.deliveries.is_empty() {
                return Err(ActivationInputError::IncompleteEntry);
            }
            entry.deliveries.sort();
            entry.deliveries.dedup();
            total_body_bytes = total_body_bytes
                .checked_add(entry.body.as_str().len())
                .ok_or(ActivationInputError::BodyLimit)?;
            if total_body_bytes > max_total_body_bytes {
                return Err(ActivationInputError::BodyLimit);
            }
            if mapped.insert(entry.source_id.clone(), entry).is_some() {
                return Err(ActivationInputError::DuplicateEntry);
            }
        }
        Ok(Self {
            entries: mapped,
            macros,
            max_macro_expansion_bytes,
        })
    }

    pub fn entry(&self, source_id: &KnowledgeSourceId) -> Option<&ActivationEntryInput> {
        self.entries.get(source_id)
    }

    pub fn macros(&self) -> &ActivationMacroValues {
        &self.macros
    }

    pub fn max_macro_expansion_bytes(&self) -> usize {
        self.max_macro_expansion_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationInputError {
    #[error("activation execution input limit must be positive")]
    InvalidLimit,
    #[error("activation execution input entry limit exceeded")]
    EntryLimit,
    #[error("activation execution input body limit exceeded")]
    BodyLimit,
    #[error("activation execution input macro limit exceeded")]
    MacroLimit,
    #[error("activation execution input entry is incomplete")]
    IncompleteEntry,
    #[error("activation execution input contains a duplicate entry")]
    DuplicateEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPatternKind {
    PrimaryLiteral,
    PrimaryRegex,
    SecondaryLiteral,
    SecondaryRegex,
    Constant,
    Sticky,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationRejectionReason {
    Disabled,
    ScopeMismatch,
    Delayed,
    Cooldown,
    RecursionExcluded,
    RecursionLevelLocked,
    SecondaryCondition,
    GroupLoser,
    Probability,
    Budget,
    Duplicate,
    WorkLimit,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationEvidence {
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_fragment_order: u32,
    pub match_count: u16,
    pub group_score_contribution: u16,
    pub round: u16,
    pub recursion_level: u16,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ActivationWorkUsage {
    pub scan_fragments: usize,
    pub scan_bytes: usize,
    pub scan_tokens: u64,
    pub pattern_matches: usize,
    pub candidate_evaluations: usize,
    pub recursion_steps: u16,
    pub recursion_fragments: usize,
    pub recursion_bytes: usize,
    pub recursion_tokens: u64,
    pub activated_entries: usize,
    pub knowledge_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ExternalActivationSeed {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub kind: ActivationSeedKind,
    pub provider_rank: Option<u32>,
    pub mandatory: bool,
}

#[derive(Debug, Clone)]
pub struct ActivationRequest<'a> {
    pub story_id: &'a StoryId,
    pub turn_number: TurnNumber,
    pub generation_trigger: GenerationTrigger,
    pub mode: ActivationRunMode,
    pub knowledge_snapshot: &'a KnowledgeSnapshotRef,
    pub index_snapshot: &'a ActivationIndexSnapshot,
    pub scan_buffer: &'a ActivationScanBuffer,
    pub timed_state: &'a [ActivationTimedState],
    pub external_seeds: &'a [ExternalActivationSeed],
    pub continuation: Option<ActivationContinuation>,
    pub limits: ActivationRuntimeLimits,
}

#[derive(Debug, Clone)]
pub struct ActivationResult {
    pub activated: Vec<ActivatedKnowledgeRef>,
    pub continuation: ActivationContinuation,
    pub pending_timed_state: PendingActivationStateDelta,
    pub rejection_summary: BTreeMap<ActivationRejectionReason, u32>,
    pub stop_reason: ActivationStopReason,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivatedKnowledgeRef {
    pub source_id: KnowledgeSourceId,
    pub deliveries: Vec<KnowledgeDelivery>,
    pub activation_class: ActivationSeedKind,
    pub rank: u32,
    pub token_cost: u64,
    pub evidence: Vec<ActivationEvidence>,
    #[serde(skip)]
    pub(crate) ordering: ActivationOrdering,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ActivationOrdering {
    pub class_rank: u8,
    pub order_rank: i32,
    pub score_rank: u16,
    pub source_priority: u8,
    pub recency_depth: u16,
    pub salience_rank: u8,
    pub provider_rank: u32,
}

#[derive(Debug, Clone)]
pub struct ActivationContinuation {
    pub turn_number: TurnNumber,
    pub generation_trigger: GenerationTrigger,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
    pub index_snapshot: ActivationIndexSnapshotRef,
    pub activated: BTreeMap<KnowledgeSourceId, ActivatedKnowledgeRef>,
    pub terminal_rejections: BTreeMap<KnowledgeSourceId, ActivationRejectionReason>,
    pub failed_probability: BTreeSet<KnowledgeSourceId>,
    pub group_winners: BTreeMap<ActivationGroupKey, KnowledgeSourceId>,
    pub recursion_level: u16,
    pub scan_depth: u16,
    pub depth_expansions: u16,
    pub recursion_sources: BTreeSet<KnowledgeSourceId>,
    pub scanned_fragment_ids: BTreeSet<ScanFragmentId>,
    pub audience_items: BTreeMap<KnowledgeDelivery, usize>,
    pub audience_tokens: BTreeMap<KnowledgeDelivery, u64>,
    pub total_delivery_items: usize,
    pub normal_tokens: u64,
    pub reserved_tokens: u64,
    pub mandatory_tokens: u64,
    pub consumed: ActivationWorkUsage,
    pub evidence_bytes: usize,
    pub limits: ActivationRuntimeLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMachineState {
    Initial,
    Recursion,
    DepthExpansion,
    Resumed,
    Complete,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivationRuntimeLimits {
    pub minimum_activations: usize,
    pub initial_scan_depth: u16,
    pub max_scan_depth: u16,
    pub include_summary_at_max_depth: bool,
    pub max_scan_fragments: usize,
    pub max_scan_bytes: usize,
    pub max_scan_tokens: u64,
    pub max_literal_patterns: usize,
    pub max_regex_patterns: usize,
    pub max_pattern_matches: usize,
    pub max_candidates_per_round: usize,
    pub max_recursion_steps: u16,
    pub max_recursion_fragments: usize,
    pub max_recursion_bytes: usize,
    pub max_recursion_tokens: u64,
    pub max_activated_entries: usize,
    pub max_depth_expansions: u16,
    pub max_external_candidates: usize,
    pub max_evidence_per_entry: usize,
    pub max_evidence_bytes: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_single_entry_bytes: usize,
    pub reserved_tokens: u64,
    pub mandatory_tokens: u64,
}
