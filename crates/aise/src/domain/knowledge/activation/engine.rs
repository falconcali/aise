use super::contracts::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEntryBody, ActivationEntryMetadata, ActivationEvidence,
    ActivationMachineState, ActivationOrdering, ActivationPatternKind, ActivationRecursionInput,
    ActivationRejectionReason, ActivationRequest, ActivationResult, ActivationRoundOutcome, ActivationWorkUsage,
};
use super::index::{FragmentPatternMatch, MATCHER_VERSION, summary_visible};
use super::rule::{ActivationBudgetClass, ActivationGroupKey, SecondaryLogic};
use super::scan::{ScanFragment, ScanFragmentKind};
use super::state::{ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::TurnNumber;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::turn::KnowledgeDelivery;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Candidate {
    source_id: KnowledgeSourceId,
    class: ActivationSeedKind,
    evidence: Vec<ActivationEvidence>,
    score: u16,
    provider_rank: Option<u32>,
    mandatory: bool,
    sticky: bool,
    recursion_only: bool,
    deliveries: Vec<KnowledgeDelivery>,
}

#[derive(Default)]
struct BudgetUsage {
    audience_items: BTreeMap<KnowledgeDelivery, usize>,
    audience_tokens: BTreeMap<KnowledgeDelivery, u64>,
    total_items: usize,
    normal_tokens: u64,
    reserved_tokens: u64,
    mandatory_tokens: u64,
}

enum AdmissionOutcome {
    Admitted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Start,
    InitialRecursion,
    DepthLoop,
    DepthRecursion,
    Done,
}

pub struct KnowledgeActivationEngine;

impl KnowledgeActivationEngine {
    pub fn start<'a>(&self, request: ActivationRequest<'a>) -> Result<KnowledgeActivationSession<'a>, ActivationError> {
        KnowledgeActivationSession::start(request)
    }
}

pub struct KnowledgeActivationSession<'a> {
    request: ActivationRequest<'a>,
    continuation: ActivationContinuation,
    timed_valid: BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    timed_delta: PendingActivationStateDelta,
    rejections: BTreeMap<ActivationRejectionReason, u32>,
    budget: BudgetUsage,
    recursion_fragments: Vec<ScanFragment>,
    recursion_matches: BTreeMap<super::scan::ScanFragmentId, Vec<FragmentPatternMatch>>,
    recursion_pending: Vec<KnowledgeSourceId>,
    recursion_bodies: BTreeMap<KnowledgeSourceId, (BoundedText, u64)>,
    pending: BTreeMap<KnowledgeSourceId, Candidate>,
    stage: Stage,
    state: ActivationMachineState,
    budget_trimmed: bool,
    recursion_exhausted: bool,
}

impl<'a> KnowledgeActivationSession<'a> {
    pub fn start(request: ActivationRequest<'a>) -> Result<Self, ActivationError> {
        validate_request(&request)?;
        let continuation = match request.continuation.as_ref() {
            Some(value) => {
                if value.turn_number != request.turn_number
                    || value.generation_trigger != request.generation_trigger
                    || value.knowledge_snapshot != *request.knowledge_snapshot
                    || value.index_snapshot != request.index_snapshot.reference
                    || value.limits != request.limits
                {
                    return Err(ActivationError::ContinuationMismatch);
                }
                value.clone()
            }
            None => new_continuation(&request),
        };
        let resumed = request.continuation.is_some();
        let timed = timed_view(&request)?;
        let budget = budget_usage(&request, &continuation)?;
        let mut rejections = BTreeMap::new();
        for reason in continuation.terminal_rejections.values() {
            increment(&mut rejections, *reason);
        }
        Ok(Self {
            request,
            continuation,
            timed_valid: timed.valid,
            timed_delta: timed.delta,
            rejections,
            budget,
            recursion_fragments: Vec::new(),
            recursion_matches: BTreeMap::new(),
            recursion_pending: Vec::new(),
            recursion_bodies: BTreeMap::new(),
            pending: BTreeMap::new(),
            stage: Stage::Start,
            state: if resumed {
                ActivationMachineState::Resumed
            } else {
                ActivationMachineState::Initial
            },
            budget_trimmed: false,
            recursion_exhausted: false,
        })
    }

    pub fn state(&self) -> ActivationMachineState {
        self.state
    }

    pub fn next_round(&mut self) -> Result<Option<ActivationRoundOutcome>, ActivationError> {
        if !self.pending.is_empty() {
            return Err(ActivationError::ContinuationMismatch);
        }
        loop {
            match self.stage {
                Stage::Done => return Ok(None),
                Stage::Start => {
                    self.account_scan_fragments()?;
                    self.stage = Stage::InitialRecursion;
                    return self.run_round(self.state).map(Some);
                }
                Stage::InitialRecursion => match self.expand_recursion()? {
                    RecursionStep::None => self.stage = Stage::DepthLoop,
                    RecursionStep::Exhausted => self.stage = Stage::DepthLoop,
                    RecursionStep::Expanded => {
                        return self.run_round(ActivationMachineState::Recursion).map(Some);
                    }
                },
                Stage::DepthRecursion => match self.expand_recursion()? {
                    RecursionStep::None => {
                        self.stage = Stage::DepthLoop;
                    }
                    RecursionStep::Exhausted => {
                        self.stage = Stage::Done;
                        return Ok(None);
                    }
                    RecursionStep::Expanded => {
                        return self.run_round(ActivationMachineState::Recursion).map(Some);
                    }
                },
                Stage::DepthLoop => {
                    let limits = self.request.limits;
                    if limits.minimum_activations > self.continuation.activated.len()
                        && self.continuation.scan_depth < limits.max_scan_depth
                        && self.continuation.depth_expansions < limits.max_depth_expansions
                    {
                        self.continuation.scan_depth = self.continuation.scan_depth.saturating_add(1);
                        self.continuation.depth_expansions = self.continuation.depth_expansions.saturating_add(1);
                        self.account_scan_fragments()?;
                        self.stage = Stage::DepthRecursion;
                        return self.run_round(ActivationMachineState::DepthExpansion).map(Some);
                    }
                    self.stage = Stage::Done;
                    return Ok(None);
                }
            }
        }
    }

    pub fn supply_bodies(&mut self, input: ActivationRecursionInput) -> Result<(), ActivationError> {
        for body in input.bodies {
            let Some(candidate) = self.pending.remove(&body.source_id) else {
                return Err(ActivationError::SnapshotMismatch);
            };
            self.admit_body(candidate, body)?;
        }
        if !self.pending.is_empty() {
            return Err(ActivationError::SnapshotMismatch);
        }
        Ok(())
    }

    pub fn admitted_is_mandatory(&self, source_id: &KnowledgeSourceId) -> Result<bool, ActivationError> {
        let candidate = self.pending.get(source_id).ok_or(ActivationError::SnapshotMismatch)?;
        let metadata = self
            .request
            .index_snapshot
            .metadata
            .get(source_id)
            .ok_or(ActivationError::SnapshotMismatch)?;
        Ok(is_mandatory(metadata.rule.budget_class, candidate.mandatory))
    }

    pub fn drop_admitted(&mut self, source_id: &KnowledgeSourceId) -> Result<(), ActivationError> {
        if self.pending.remove(source_id).is_none() {
            return Err(ActivationError::SnapshotMismatch);
        }
        reject(
            &mut self.continuation,
            &mut self.rejections,
            source_id.clone(),
            ActivationRejectionReason::Budget,
        );
        self.budget_trimmed = true;
        Ok(())
    }

    pub fn finish(mut self) -> Result<ActivationResult, ActivationError> {
        if !self.pending.is_empty() {
            return Err(ActivationError::ContinuationMismatch);
        }
        cleanup_timed_state(&self.request, &self.continuation, &self.timed_valid, &mut self.timed_delta);
        self.continuation.consumed.knowledge_tokens = self
            .budget
            .normal_tokens
            .saturating_add(self.budget.reserved_tokens)
            .saturating_add(self.budget.mandatory_tokens);
        self.continuation.audience_items = self.budget.audience_items.clone();
        self.continuation.audience_tokens = self.budget.audience_tokens.clone();
        self.continuation.total_delivery_items = self.budget.total_items;
        self.continuation.normal_tokens = self.budget.normal_tokens;
        self.continuation.reserved_tokens = self.budget.reserved_tokens;
        self.continuation.mandatory_tokens = self.budget.mandatory_tokens;
        let activated = ranked_activated(&mut self.continuation);
        let limits = self.request.limits;
        let stop_reason = if self.recursion_exhausted {
            ActivationStopReason::RecursionExhausted
        } else if self.budget_trimmed {
            ActivationStopReason::WorkTrimmed
        } else if limits.minimum_activations > activated.len()
            && (self.continuation.scan_depth == limits.max_scan_depth
                || self.continuation.depth_expansions == limits.max_depth_expansions)
        {
            ActivationStopReason::MaximumDepthReached
        } else if self.continuation.depth_expansions > 0 && activated.len() >= limits.minimum_activations {
            ActivationStopReason::MinimumSatisfied
        } else {
            ActivationStopReason::Complete
        };
        self.state = ActivationMachineState::Complete;
        Ok(ActivationResult {
            activated,
            continuation: self.continuation,
            pending_timed_state: self.timed_delta,
            rejection_summary: self.rejections,
            stop_reason,
        })
    }

    fn run_round(&mut self, state: ActivationMachineState) -> Result<ActivationRoundOutcome, ActivationError> {
        self.state = state;
        let admitted = self.select_round()?;
        Ok(ActivationRoundOutcome { admitted, state })
    }

    fn account_scan_fragments(&mut self) -> Result<(), ActivationError> {
        let limits = self.request.limits;
        for fragment in visible_base_fragments(&self.request, self.continuation.scan_depth) {
            if !self.continuation.scanned_fragment_ids.insert(fragment.id.clone()) {
                continue;
            }
            self.continuation.consumed.scan_fragments = self.continuation.consumed.scan_fragments.saturating_add(1);
            self.continuation.consumed.scan_bytes = self
                .continuation
                .consumed
                .scan_bytes
                .saturating_add(fragment.text.as_str().len());
            self.continuation.consumed.scan_tokens = self
                .continuation
                .consumed
                .scan_tokens
                .saturating_add(bytes_to_tokens(fragment.text.as_str().len()));
        }
        if self.continuation.consumed.scan_fragments > limits.max_scan_fragments {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "scan_fragments",
            });
        }
        if self.continuation.consumed.scan_bytes > limits.max_scan_bytes {
            return Err(ActivationError::WorkLimitExceeded { limit: "scan_bytes" });
        }
        if self.continuation.consumed.scan_tokens > limits.max_scan_tokens {
            return Err(ActivationError::WorkLimitExceeded { limit: "scan_tokens" });
        }
        Ok(())
    }

    fn select_round(&mut self) -> Result<Vec<KnowledgeSourceId>, ActivationError> {
        let mut candidates = self.collect_candidates()?;
        if candidates.len() > self.request.limits.max_candidates_per_round {
            let mut ordered = candidates.values().cloned().collect::<Vec<_>>();
            ordered.sort_by(|left, right| {
                let left_mandatory = self
                    .request
                    .index_snapshot
                    .metadata
                    .get(&left.source_id)
                    .is_some_and(|metadata| is_mandatory(metadata.rule.budget_class, left.mandatory));
                let right_mandatory = self
                    .request
                    .index_snapshot
                    .metadata
                    .get(&right.source_id)
                    .is_some_and(|metadata| is_mandatory(metadata.rule.budget_class, right.mandatory));
                right_mandatory
                    .cmp(&left_mandatory)
                    .then_with(|| candidate_order(&self.request, left, right))
            });
            let mandatory_count = ordered
                .iter()
                .take_while(|candidate| {
                    self.request
                        .index_snapshot
                        .metadata
                        .get(&candidate.source_id)
                        .is_some_and(|metadata| is_mandatory(metadata.rule.budget_class, candidate.mandatory))
                })
                .count();
            if mandatory_count > self.request.limits.max_candidates_per_round {
                return Err(ActivationError::MandatoryBudgetExceeded);
            }
            let retained = ordered
                .iter()
                .take(self.request.limits.max_candidates_per_round)
                .map(|candidate| candidate.source_id.clone())
                .collect::<BTreeSet<_>>();
            let rejected = candidates
                .keys()
                .filter(|source_id| !retained.contains(*source_id))
                .cloned()
                .collect::<Vec<_>>();
            for source_id in rejected {
                candidates.remove(&source_id);
                reject(
                    &mut self.continuation,
                    &mut self.rejections,
                    source_id,
                    ActivationRejectionReason::WorkLimit,
                );
            }
            self.budget_trimmed = true;
        }
        let mut eligible = Vec::new();
        for candidate in candidates.values_mut() {
            self.continuation.consumed.candidate_evaluations =
                self.continuation.consumed.candidate_evaluations.saturating_add(1);
            let metadata = self
                .request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .ok_or(ActivationError::ExternalTargetUnauthorized)?
                .clone();
            if self.continuation.activated.contains_key(&candidate.source_id) {
                self.merge_existing_deliveries(&metadata, candidate)?;
                continue;
            }
            if self.continuation.terminal_rejections.contains_key(&candidate.source_id) {
                continue;
            }
            if let Some(reason) = self.pre_filter(&metadata, candidate) {
                match reason {
                    ActivationRejectionReason::RecursionLevelLocked => {
                        increment(&mut self.rejections, reason);
                    }
                    other => reject(&mut self.continuation, &mut self.rejections, candidate.source_id.clone(), other),
                }
                continue;
            }
            eligible.push(candidate.clone());
        }

        eligible = resolve_groups(&self.request, &mut self.continuation, &mut self.rejections, eligible);
        eligible.sort_by(|left, right| candidate_order(&self.request, left, right));
        let mut admitted = Vec::new();
        for candidate in eligible {
            let metadata = self
                .request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .ok_or(ActivationError::ExternalTargetUnauthorized)?
                .clone();
            if !candidate.sticky
                && !probability_admits(
                    self.request.story_id.as_str(),
                    self.request.turn_number.get(),
                    &candidate.source_id,
                    metadata.rule.selection.probability,
                    metadata.rule_version.as_digest().as_bytes(),
                )
            {
                self.continuation.failed_probability.insert(candidate.source_id.clone());
                reject(
                    &mut self.continuation,
                    &mut self.rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Probability,
                );
                continue;
            }
            if self.continuation.activated.len().saturating_add(self.pending.len())
                >= self.request.limits.max_activated_entries
            {
                if is_mandatory(metadata.rule.budget_class, candidate.mandatory) {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                reject(
                    &mut self.continuation,
                    &mut self.rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Budget,
                );
                self.budget_trimmed = true;
                continue;
            }
            admitted.push(candidate.source_id.clone());
            self.pending.insert(candidate.source_id.clone(), candidate);
        }
        Ok(admitted)
    }

    fn merge_existing_deliveries(
        &mut self,
        metadata: &ActivationEntryMetadata,
        candidate: &Candidate,
    ) -> Result<(), ActivationError> {
        let Some(existing) = self.continuation.activated.get(&candidate.source_id) else {
            return Ok(());
        };
        let token_cost = existing.token_cost;
        let new_deliveries = candidate
            .deliveries
            .iter()
            .filter(|delivery| !existing.deliveries.contains(delivery))
            .cloned()
            .collect::<Vec<_>>();
        if new_deliveries.is_empty() {
            return Ok(());
        }
        validate_deliveries(metadata.kind, &new_deliveries)?;
        match admit_deliveries(
            &self.request,
            metadata.rule.budget_class,
            candidate.mandatory,
            token_cost,
            &new_deliveries,
            &mut self.budget,
        ) {
            AdmissionOutcome::Admitted => {}
            AdmissionOutcome::Rejected => {
                if is_mandatory(metadata.rule.budget_class, candidate.mandatory) {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                increment(&mut self.rejections, ActivationRejectionReason::Budget);
                self.budget_trimmed = true;
                return Ok(());
            }
        }
        if let Some(existing) = self.continuation.activated.get_mut(&candidate.source_id) {
            existing.deliveries.extend(new_deliveries);
            existing.deliveries.sort();
            existing.deliveries.dedup();
        }
        Ok(())
    }

    fn pre_filter(
        &self,
        metadata: &ActivationEntryMetadata,
        candidate: &Candidate,
    ) -> Option<ActivationRejectionReason> {
        if !metadata.rule.mode.enabled {
            return Some(ActivationRejectionReason::Disabled);
        }
        if !metadata.rule.scope.generation_triggers.is_empty()
            && !metadata
                .rule
                .scope
                .generation_triggers
                .contains(&self.request.generation_trigger)
        {
            return Some(ActivationRejectionReason::ScopeMismatch);
        }
        if metadata.rule.timing.delay_turns > 0
            && self.request.turn_number.get() <= u64::from(metadata.rule.timing.delay_turns)
        {
            return Some(ActivationRejectionReason::Delayed);
        }
        if !candidate.sticky
            && self.timed_valid.get(&candidate.source_id).is_some_and(|state| {
                state
                    .cooldown_through_turn
                    .is_some_and(|through| through >= self.request.turn_number)
            })
        {
            return Some(ActivationRejectionReason::Cooldown);
        }
        if !candidate.sticky && candidate.recursion_only && metadata.rule.recursion.exclude_recursion {
            return Some(ActivationRejectionReason::RecursionExcluded);
        }
        if !candidate.sticky
            && metadata
                .rule
                .recursion
                .delay_until_recursion
                .is_some_and(|level| self.continuation.recursion_level < level)
        {
            return Some(ActivationRejectionReason::RecursionLevelLocked);
        }
        None
    }

    fn admit_body(&mut self, candidate: Candidate, body: ActivationEntryBody) -> Result<(), ActivationError> {
        let metadata = self
            .request
            .index_snapshot
            .metadata
            .get(&body.source_id)
            .ok_or(ActivationError::IndexVersionMismatch)?
            .clone();
        if metadata.kind != body.kind || body.token_cost == 0 {
            return Err(ActivationError::SnapshotMismatch);
        }
        let mandatory = is_mandatory(metadata.rule.budget_class, candidate.mandatory);
        if body.body.as_str().len() > self.request.limits.max_single_entry_bytes {
            if mandatory {
                return Err(ActivationError::MandatoryBudgetExceeded);
            }
            reject(
                &mut self.continuation,
                &mut self.rejections,
                body.source_id,
                ActivationRejectionReason::Budget,
            );
            self.budget_trimmed = true;
            return Ok(());
        }
        let ordering = activation_ordering(&self.request, &candidate);
        let mut evidence = candidate.evidence;
        evidence.sort_by_key(|item| {
            (
                item.round,
                item.recursion_level,
                source_priority(item.fragment_kind),
                item.recency_depth,
                item.stable_fragment_order,
                item.pattern_kind,
                item.pattern_ordinal,
            )
        });
        evidence.truncate(self.request.limits.max_evidence_per_entry);
        let evidence_bytes = evidence.len().saturating_mul(std::mem::size_of::<ActivationEvidence>());
        if self.continuation.evidence_bytes.saturating_add(evidence_bytes) > self.request.limits.max_evidence_bytes {
            if mandatory {
                return Err(ActivationError::MandatoryBudgetExceeded);
            }
            reject(
                &mut self.continuation,
                &mut self.rejections,
                body.source_id,
                ActivationRejectionReason::WorkLimit,
            );
            self.budget_trimmed = true;
            return Ok(());
        }
        let mut deliveries = default_deliveries(metadata.kind);
        deliveries.extend(candidate.deliveries.clone());
        deliveries.sort();
        deliveries.dedup();
        validate_deliveries(metadata.kind, &deliveries)?;
        match admit_deliveries(
            &self.request,
            metadata.rule.budget_class,
            candidate.mandatory,
            body.token_cost,
            &deliveries,
            &mut self.budget,
        ) {
            AdmissionOutcome::Admitted => {}
            AdmissionOutcome::Rejected => {
                if mandatory {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                reject(
                    &mut self.continuation,
                    &mut self.rejections,
                    body.source_id,
                    ActivationRejectionReason::Budget,
                );
                self.budget_trimmed = true;
                return Ok(());
            }
        }
        self.continuation.evidence_bytes = self.continuation.evidence_bytes.saturating_add(evidence_bytes);
        let source_id = body.source_id.clone();
        self.continuation.activated.insert(
            source_id.clone(),
            ActivatedKnowledgeRef {
                source_id: source_id.clone(),
                deliveries,
                activation_class: candidate.class,
                rank: 0,
                token_cost: body.token_cost,
                evidence,
                ordering,
            },
        );
        self.continuation.consumed.activated_entries = self.continuation.consumed.activated_entries.saturating_add(1);
        if !candidate.sticky {
            update_timed_state(&self.request, &metadata, &mut self.timed_delta);
        }
        self.recursion_bodies.insert(source_id.clone(), (body.body, body.token_cost));
        self.recursion_pending.push(source_id);
        Ok(())
    }

    fn expand_recursion(&mut self) -> Result<RecursionStep, ActivationError> {
        let added = self.add_recursion_fragments()?;
        if added == 0 {
            return Ok(RecursionStep::None);
        }
        if self.continuation.recursion_level >= self.request.limits.max_recursion_steps {
            self.recursion_exhausted = true;
            return Ok(RecursionStep::Exhausted);
        }
        self.continuation.recursion_level = self.continuation.recursion_level.saturating_add(1);
        self.continuation.consumed.recursion_steps = self.continuation.consumed.recursion_steps.saturating_add(1);
        Ok(RecursionStep::Expanded)
    }

    fn add_recursion_fragments(&mut self) -> Result<usize, ActivationError> {
        self.recursion_pending.sort();
        self.recursion_pending.dedup();
        let mut added = 0usize;
        for source_id in std::mem::take(&mut self.recursion_pending) {
            let body = self.recursion_bodies.remove(&source_id);
            if self.continuation.recursion_sources.contains(&source_id) {
                continue;
            }
            let metadata = self
                .request
                .index_snapshot
                .metadata
                .get(&source_id)
                .ok_or(ActivationError::IndexVersionMismatch)?
                .clone();
            self.continuation.recursion_sources.insert(source_id.clone());
            if metadata.rule.recursion.prevent_recursion {
                continue;
            }
            let Some((text, token_cost)) = body else {
                return Err(ActivationError::SnapshotMismatch);
            };
            let bytes = text.as_str().len();
            let limits = self.request.limits;
            if self.continuation.consumed.recursion_fragments.saturating_add(1) > limits.max_recursion_fragments
                || self.continuation.consumed.recursion_bytes.saturating_add(bytes) > limits.max_recursion_bytes
                || self.continuation.consumed.recursion_tokens.saturating_add(token_cost) > limits.max_recursion_tokens
            {
                return Err(ActivationError::RecursionLimitReached);
            }
            let stable_order = u32::try_from(self.recursion_fragments.len()).unwrap_or(u32::MAX);
            let fragment = ScanFragment::new(
                ScanFragmentKind::RecursionContent,
                self.continuation.recursion_level.saturating_add(1),
                stable_order,
                text,
            );
            let matches = self.request.index_snapshot.match_fragment(&fragment);
            self.recursion_matches.insert(fragment.id.clone(), matches);
            self.recursion_fragments.push(fragment);
            self.continuation.consumed.recursion_fragments =
                self.continuation.consumed.recursion_fragments.saturating_add(1);
            self.continuation.consumed.recursion_bytes =
                self.continuation.consumed.recursion_bytes.saturating_add(bytes);
            self.continuation.consumed.recursion_tokens =
                self.continuation.consumed.recursion_tokens.saturating_add(token_cost);
            added = added.saturating_add(1);
        }
        Ok(added)
    }

    fn collect_candidates(&mut self) -> Result<BTreeMap<KnowledgeSourceId, Candidate>, ActivationError> {
        let mut candidates = BTreeMap::new();
        for source_id in &self.request.index_snapshot.constant_entries {
            let metadata = self
                .request
                .index_snapshot
                .metadata
                .get(source_id)
                .ok_or(ActivationError::IndexVersionMismatch)?;
            merge_candidate(
                &mut candidates,
                Candidate {
                    source_id: source_id.clone(),
                    class: ActivationSeedKind::Constant,
                    evidence: Vec::new(),
                    score: 0,
                    provider_rank: None,
                    mandatory: metadata.rule.budget_class == ActivationBudgetClass::Mandatory,
                    sticky: false,
                    recursion_only: false,
                    deliveries: Vec::new(),
                },
            );
        }
        for (source_id, state) in &self.timed_valid {
            if state
                .sticky_through_turn
                .is_some_and(|through| through >= self.request.turn_number)
            {
                merge_candidate(
                    &mut candidates,
                    Candidate {
                        source_id: source_id.clone(),
                        class: ActivationSeedKind::Sticky,
                        evidence: Vec::new(),
                        score: 0,
                        provider_rank: None,
                        mandatory: false,
                        sticky: true,
                        recursion_only: false,
                        deliveries: Vec::new(),
                    },
                );
            }
        }
        for seed in self.request.external_seeds {
            if !self.request.index_snapshot.metadata.contains_key(&seed.source_id) {
                return Err(ActivationError::ExternalTargetUnauthorized);
            }
            merge_candidate(
                &mut candidates,
                Candidate {
                    source_id: seed.source_id.clone(),
                    class: seed.kind,
                    evidence: Vec::new(),
                    score: 0,
                    provider_rank: seed.provider_rank,
                    mandatory: seed.mandatory,
                    sticky: false,
                    recursion_only: false,
                    deliveries: vec![seed.delivery.clone()],
                },
            );
        }
        self.collect_text_candidates(&mut candidates)?;
        Ok(candidates)
    }

    fn collect_text_candidates(
        &mut self,
        candidates: &mut BTreeMap<KnowledgeSourceId, Candidate>,
    ) -> Result<(), ActivationError> {
        let round = self
            .continuation
            .depth_expansions
            .saturating_add(self.continuation.recursion_level);
        let recursion_level = self.continuation.recursion_level;
        let mut by_source: BTreeMap<KnowledgeSourceId, Vec<FragmentPatternMatch>> = BTreeMap::new();
        for fragment in visible_base_fragments(&self.request, self.continuation.scan_depth) {
            let matches = self
                .request
                .fragment_matches
                .get(&fragment.id)
                .ok_or(ActivationError::IndexVersionMismatch)?;
            for entry in matches.iter() {
                by_source.entry(entry.source_id.clone()).or_default().push(entry.clone());
            }
        }
        for fragment in &self.recursion_fragments {
            let Some(matches) = self.recursion_matches.get(&fragment.id) else {
                return Err(ActivationError::IndexVersionMismatch);
            };
            for entry in matches {
                by_source.entry(entry.source_id.clone()).or_default().push(entry.clone());
            }
        }
        let limits = self.request.limits;
        for (source_id, metadata) in &self.request.index_snapshot.metadata {
            if !metadata.rule.mode.enabled || metadata.rule.mode.constant || metadata.rule.mode.exact_target_only {
                continue;
            }
            let entry_depth = metadata
                .rule
                .match_rule
                .scan_depth
                .unwrap_or(self.continuation.scan_depth)
                .min(self.continuation.scan_depth);
            let Some(entry_matches) = by_source.get(source_id) else {
                continue;
            };
            let mut match_count_total = 0usize;
            let mut primary_ordinals = BTreeSet::new();
            let mut secondary_ordinals = BTreeSet::new();
            let mut evidence = Vec::new();
            for item in entry_matches {
                if item.fragment_kind != ScanFragmentKind::RecursionContent {
                    if item.recency_depth > entry_depth {
                        continue;
                    }
                    if !summary_visible(
                        item.fragment_kind,
                        entry_depth,
                        limits.include_summary_at_max_depth,
                        limits.max_scan_depth,
                    ) {
                        continue;
                    }
                }
                match_count_total = match_count_total.saturating_add(usize::from(item.match_count));
                match item.pattern_kind {
                    ActivationPatternKind::PrimaryLiteral | ActivationPatternKind::PrimaryRegex => {
                        primary_ordinals.insert(item.pattern_ordinal);
                    }
                    ActivationPatternKind::SecondaryLiteral | ActivationPatternKind::SecondaryRegex => {
                        secondary_ordinals.insert(item.pattern_ordinal);
                    }
                    _ => {}
                }
                evidence.push(ActivationEvidence {
                    pattern_kind: item.pattern_kind,
                    pattern_ordinal: item.pattern_ordinal,
                    fragment_kind: item.fragment_kind,
                    recency_depth: item.recency_depth,
                    stable_fragment_order: item.stable_fragment_order,
                    match_count: item.match_count,
                    group_score_contribution: item.group_score_contribution,
                    round,
                    recursion_level,
                });
            }
            self.continuation.consumed.pattern_matches =
                self.continuation.consumed.pattern_matches.saturating_add(match_count_total);
            if self.continuation.consumed.pattern_matches > limits.max_pattern_matches {
                if metadata.rule.budget_class == ActivationBudgetClass::Mandatory {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                reject(
                    &mut self.continuation,
                    &mut self.rejections,
                    source_id.clone(),
                    ActivationRejectionReason::WorkLimit,
                );
                self.budget_trimmed = true;
                continue;
            }
            if primary_ordinals.is_empty() {
                continue;
            }
            if !secondary_logic(
                metadata.rule.match_rule.secondary_logic,
                &secondary_ordinals,
                metadata.rule.match_rule.secondary_keys.len(),
            ) {
                increment(&mut self.rejections, ActivationRejectionReason::SecondaryCondition);
                continue;
            }
            let recursion_only = evidence
                .iter()
                .filter(|item| {
                    matches!(
                        item.pattern_kind,
                        ActivationPatternKind::PrimaryLiteral | ActivationPatternKind::PrimaryRegex
                    )
                })
                .all(|item| item.fragment_kind == ScanFragmentKind::RecursionContent);
            merge_candidate(
                candidates,
                Candidate {
                    source_id: source_id.clone(),
                    class: if recursion_only {
                        ActivationSeedKind::Recursion
                    } else {
                        ActivationSeedKind::TextMatch
                    },
                    score: u16::try_from(primary_ordinals.len().saturating_add(secondary_ordinals.len()))
                        .unwrap_or(u16::MAX),
                    evidence,
                    provider_rank: None,
                    mandatory: metadata.rule.budget_class == ActivationBudgetClass::Mandatory,
                    sticky: false,
                    recursion_only,
                    deliveries: Vec::new(),
                },
            );
        }
        Ok(())
    }
}

enum RecursionStep {
    None,
    Expanded,
    Exhausted,
}

struct TimedView {
    valid: BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    delta: PendingActivationStateDelta,
}

fn validate_request(request: &ActivationRequest<'_>) -> Result<(), ActivationError> {
    let limits = request.limits;
    let positive = [
        limits.max_scan_fragments,
        limits.max_scan_bytes,
        usize::try_from(limits.max_scan_tokens).unwrap_or(usize::MAX),
        limits.max_literal_patterns,
        limits.max_regex_patterns,
        limits.max_pattern_matches,
        limits.max_candidates_per_round,
        usize::from(limits.max_recursion_steps),
        limits.max_recursion_fragments,
        limits.max_recursion_bytes,
        usize::try_from(limits.max_recursion_tokens).unwrap_or(usize::MAX),
        limits.max_activated_entries,
        usize::from(limits.max_depth_expansions),
        limits.max_external_candidates,
        limits.max_evidence_per_entry,
        limits.max_evidence_bytes,
        limits.max_items_per_audience,
        usize::try_from(limits.max_tokens_per_audience).unwrap_or(usize::MAX),
        limits.max_total_items,
        usize::try_from(limits.max_total_tokens).unwrap_or(usize::MAX),
        limits.max_single_entry_bytes,
    ];
    if positive.into_iter().any(|value| value == 0)
        || limits.initial_scan_depth > limits.max_scan_depth
        || limits.max_items_per_audience > limits.max_total_items
        || limits.max_tokens_per_audience > limits.max_total_tokens
        || limits.reserved_tokens > limits.max_total_tokens
        || limits.mandatory_tokens > limits.max_total_tokens
        || limits.reserved_tokens.saturating_add(limits.mandatory_tokens) > limits.max_total_tokens
        || request.external_seeds.len() > limits.max_external_candidates
    {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "runtime_limits",
        });
    }
    if !request
        .index_snapshot
        .matches_snapshot(request.knowledge_snapshot, MATCHER_VERSION)
    {
        return Err(ActivationError::SnapshotMismatch);
    }
    for metadata in request.index_snapshot.metadata.values() {
        if metadata
            .rule
            .match_rule
            .scan_depth
            .is_some_and(|depth| depth > limits.max_scan_depth)
        {
            return Err(ActivationError::InvalidRule {
                code: "scan_depth_exceeded",
            });
        }
    }
    if request.index_snapshot.literal_pattern_count() > limits.max_literal_patterns {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "literal_patterns",
        });
    }
    if request.index_snapshot.regex_pattern_count() > limits.max_regex_patterns {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "regex_patterns",
        });
    }
    Ok(())
}

fn new_continuation(request: &ActivationRequest<'_>) -> ActivationContinuation {
    ActivationContinuation {
        turn_number: request.turn_number,
        generation_trigger: request.generation_trigger,
        knowledge_snapshot: request.knowledge_snapshot.clone(),
        index_snapshot: request.index_snapshot.reference.clone(),
        activated: BTreeMap::new(),
        terminal_rejections: BTreeMap::new(),
        failed_probability: BTreeSet::new(),
        group_winners: BTreeMap::new(),
        recursion_level: 0,
        scan_depth: request.limits.initial_scan_depth,
        depth_expansions: 0,
        recursion_sources: BTreeSet::new(),
        scanned_fragment_ids: BTreeSet::new(),
        audience_items: BTreeMap::new(),
        audience_tokens: BTreeMap::new(),
        total_delivery_items: 0,
        normal_tokens: 0,
        reserved_tokens: 0,
        mandatory_tokens: 0,
        consumed: ActivationWorkUsage::default(),
        evidence_bytes: 0,
        limits: request.limits,
    }
}

fn timed_view(request: &ActivationRequest<'_>) -> Result<TimedView, ActivationError> {
    let mut valid = BTreeMap::new();
    let mut delta = PendingActivationStateDelta::default();
    for state in request.timed_state {
        if valid.contains_key(&state.source_id) {
            return Err(ActivationError::TimedStateInconsistent);
        }
        let Some(metadata) = request.index_snapshot.metadata.get(&state.source_id) else {
            delta.deletes.insert(state.source_id.clone());
            continue;
        };
        if state.rule_version != metadata.rule_version {
            delta.deletes.insert(state.source_id.clone());
            continue;
        }
        if state
            .sticky_through_turn
            .zip(state.cooldown_through_turn)
            .is_some_and(|(sticky, cooldown)| cooldown < sticky)
        {
            return Err(ActivationError::TimedStateInconsistent);
        }
        valid.insert(state.source_id.clone(), state.clone());
    }
    Ok(TimedView { valid, delta })
}

fn visible_base_fragments<'a>(request: &'a ActivationRequest<'_>, depth: u16) -> Vec<&'a ScanFragment> {
    request
        .scan_buffer
        .visible_at_depth(depth)
        .filter(|fragment| {
            fragment.kind != ScanFragmentKind::RecursionContent
                && summary_visible(
                    fragment.kind,
                    depth,
                    request.limits.include_summary_at_max_depth,
                    request.limits.max_scan_depth,
                )
        })
        .collect()
}

fn default_deliveries(kind: KnowledgeKind) -> Vec<KnowledgeDelivery> {
    match kind {
        KnowledgeKind::Fact | KnowledgeKind::Rumor => vec![KnowledgeDelivery::Writer],
        KnowledgeKind::Memory => Vec::new(),
    }
}

fn secondary_logic(logic: SecondaryLogic, matches: &BTreeSet<u16>, pattern_count: usize) -> bool {
    if pattern_count == 0 {
        return true;
    }
    match logic {
        SecondaryLogic::AndAny => !matches.is_empty(),
        SecondaryLogic::AndAll => matches.len() == pattern_count,
        SecondaryLogic::NotAny => matches.is_empty(),
        SecondaryLogic::NotAll => matches.len() < pattern_count,
    }
}

fn merge_candidate(candidates: &mut BTreeMap<KnowledgeSourceId, Candidate>, incoming: Candidate) {
    let Some(current) = candidates.get_mut(&incoming.source_id) else {
        candidates.insert(incoming.source_id.clone(), incoming);
        return;
    };
    if class_rank(&incoming) < class_rank(current) {
        current.class = incoming.class;
    }
    current.evidence.extend(incoming.evidence);
    current.score = current.score.max(incoming.score);
    current.provider_rank = match (current.provider_rank, incoming.provider_rank) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    current.mandatory |= incoming.mandatory;
    current.sticky |= incoming.sticky;
    current.recursion_only &= incoming.recursion_only;
    current.deliveries.extend(incoming.deliveries);
    current.deliveries.sort();
    current.deliveries.dedup();
}

fn resolve_groups(
    request: &ActivationRequest<'_>,
    continuation: &mut ActivationContinuation,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
    candidates: Vec<Candidate>,
) -> Vec<Candidate> {
    let grouped = candidates
        .iter()
        .filter(|candidate| {
            request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .is_some_and(|metadata| !metadata.rule.selection.groups.is_empty())
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut eligible = candidates;
    loop {
        let mut winners = continuation.group_winners.clone();
        let groups = grouped
            .iter()
            .filter(|candidate| eligible.iter().any(|item| item.source_id == candidate.source_id))
            .flat_map(|candidate| {
                request
                    .index_snapshot
                    .metadata
                    .get(&candidate.source_id)
                    .into_iter()
                    .flat_map(|metadata| metadata.rule.selection.groups.iter().cloned())
            })
            .collect::<BTreeSet<_>>();
        for group in groups {
            if winners.contains_key(&group) {
                continue;
            }
            let choices = eligible
                .iter()
                .filter(|candidate| {
                    request
                        .index_snapshot
                        .metadata
                        .get(&candidate.source_id)
                        .is_some_and(|metadata| metadata.rule.selection.groups.contains(&group))
                })
                .collect::<Vec<_>>();
            if let Some(winner) = group_winner(request, &group, &choices) {
                winners.insert(group, winner.source_id.clone());
            }
        }
        let retained = eligible
            .iter()
            .filter(|candidate| {
                request
                    .index_snapshot
                    .metadata
                    .get(&candidate.source_id)
                    .is_none_or(|metadata| {
                        metadata
                            .rule
                            .selection
                            .groups
                            .iter()
                            .all(|group| winners.get(group) == Some(&candidate.source_id))
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if retained.len() == eligible.len() {
            continuation.group_winners = winners;
            break;
        }
        eligible = retained;
    }
    let eligible_ids = eligible
        .iter()
        .map(|candidate| candidate.source_id.clone())
        .collect::<BTreeSet<_>>();
    for candidate in grouped {
        if !eligible_ids.contains(&candidate.source_id) {
            reject(
                continuation,
                rejections,
                candidate.source_id,
                ActivationRejectionReason::GroupLoser,
            );
        }
    }
    eligible
}

fn group_winner<'a>(
    request: &ActivationRequest<'_>,
    group: &ActivationGroupKey,
    choices: &[&'a Candidate],
) -> Option<&'a Candidate> {
    let best_key = choices
        .iter()
        .filter_map(|candidate| {
            request.index_snapshot.metadata.get(&candidate.source_id).map(|metadata| {
                (
                    candidate.sticky,
                    if metadata.rule.selection.use_group_scoring {
                        candidate.score
                    } else {
                        0
                    },
                    metadata.rule.selection.group_override,
                    metadata.rule.selection.order,
                )
            })
        })
        .max()?;
    let mut tied = choices
        .iter()
        .copied()
        .filter(|candidate| {
            request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .is_some_and(|metadata| {
                    (
                        candidate.sticky,
                        if metadata.rule.selection.use_group_scoring {
                            candidate.score
                        } else {
                            0
                        },
                        metadata.rule.selection.group_override,
                        metadata.rule.selection.order,
                    ) == best_key
                })
        })
        .collect::<Vec<_>>();
    tied.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    let total_weight = tied.iter().fold(0u64, |total, candidate| {
        total.saturating_add(
            request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .map_or(0, |metadata| u64::from(metadata.rule.selection.group_weight)),
        )
    });
    if total_weight == 0 {
        return tied.first().copied();
    }
    let mut hasher = Sha256::new();
    hasher.update(b"aise.knowledge.activation.group.v1");
    hasher.update(request.story_id.as_str().as_bytes());
    hasher.update(request.turn_number.get().to_be_bytes());
    hasher.update(group.as_str().as_bytes());
    for candidate in &tied {
        let metadata = request.index_snapshot.metadata.get(&candidate.source_id)?;
        hasher.update(candidate.source_id.as_str().as_bytes());
        hasher.update(metadata.rule_version.as_digest().as_bytes());
        hasher.update(metadata.rule.selection.group_weight.to_be_bytes());
    }
    let digest = hasher.finalize();
    let mut sample_bytes = [0u8; 8];
    sample_bytes.copy_from_slice(&digest[..8]);
    let mut sample = u64::from_be_bytes(sample_bytes) % total_weight;
    for candidate in tied {
        let weight = request
            .index_snapshot
            .metadata
            .get(&candidate.source_id)
            .map_or(0, |metadata| u64::from(metadata.rule.selection.group_weight));
        if sample < weight {
            return Some(candidate);
        }
        sample -= weight;
    }
    None
}

fn candidate_order(request: &ActivationRequest<'_>, left: &Candidate, right: &Candidate) -> Ordering {
    activation_ordering(request, left)
        .cmp(&activation_ordering(request, right))
        .then_with(|| left.source_id.cmp(&right.source_id))
}

fn activation_ordering(request: &ActivationRequest<'_>, candidate: &Candidate) -> ActivationOrdering {
    let metadata = request.index_snapshot.metadata.get(&candidate.source_id);
    let best_evidence = candidate.evidence.iter().min_by_key(|evidence| {
        (
            source_priority(evidence.fragment_kind),
            evidence.recency_depth,
            evidence.stable_fragment_order,
        )
    });
    ActivationOrdering {
        class_rank: class_rank(candidate),
        order_rank: metadata.map_or(0, |entry| entry.rule.selection.order).saturating_neg(),
        score_rank: u16::MAX.saturating_sub(candidate.score),
        source_priority: best_evidence.map_or(u8::MAX, |evidence| source_priority(evidence.fragment_kind)),
        recency_depth: best_evidence.map_or(u16::MAX, |evidence| evidence.recency_depth),
        salience_rank: u8::MAX.saturating_sub(metadata.map_or(0, |entry| entry.salience)),
        provider_rank: candidate.provider_rank.unwrap_or(u32::MAX),
    }
}

fn class_rank(candidate: &Candidate) -> u8 {
    if candidate.sticky {
        0
    } else if candidate.mandatory
        && matches!(
            candidate.class,
            ActivationSeedKind::PlannerExactTarget | ActivationSeedKind::PreviewOverride
        )
    {
        1
    } else if candidate.mandatory && candidate.class == ActivationSeedKind::Constant {
        2
    } else {
        match candidate.class {
            ActivationSeedKind::Constant => 3,
            ActivationSeedKind::TextMatch | ActivationSeedKind::Recursion => 4,
            ActivationSeedKind::Provider => 5,
            ActivationSeedKind::PlannerExactTarget | ActivationSeedKind::PreviewOverride => 4,
            ActivationSeedKind::Sticky => 0,
        }
    }
}

fn probability_admits(
    story_id: &str,
    turn: u64,
    source_id: &KnowledgeSourceId,
    probability: u8,
    version: &[u8; 32],
) -> bool {
    if probability == 0 {
        return false;
    }
    if probability == 100 {
        return true;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"aise.knowledge.activation.probability.v1");
    hasher.update(story_id.as_bytes());
    hasher.update(turn.to_be_bytes());
    hasher.update(source_id.as_str().as_bytes());
    hasher.update(version);
    let digest = hasher.finalize();
    let sample = u16::from_be_bytes([digest[0], digest[1]]) % 100;
    sample < u16::from(probability)
}

fn budget_usage(
    request: &ActivationRequest<'_>,
    continuation: &ActivationContinuation,
) -> Result<BudgetUsage, ActivationError> {
    if continuation.total_delivery_items > 0 || continuation.activated.is_empty() {
        return Ok(BudgetUsage {
            audience_items: continuation.audience_items.clone(),
            audience_tokens: continuation.audience_tokens.clone(),
            total_items: continuation.total_delivery_items,
            normal_tokens: continuation.normal_tokens,
            reserved_tokens: continuation.reserved_tokens,
            mandatory_tokens: continuation.mandatory_tokens,
        });
    }
    let mut usage = BudgetUsage::default();
    for activated in continuation.activated.values() {
        let metadata = request
            .index_snapshot
            .metadata
            .get(&activated.source_id)
            .ok_or(ActivationError::ContinuationMismatch)?;
        for delivery in &activated.deliveries {
            *usage.audience_items.entry(delivery.clone()).or_default() += 1;
            *usage.audience_tokens.entry(delivery.clone()).or_default() = usage
                .audience_tokens
                .get(delivery)
                .copied()
                .unwrap_or_default()
                .saturating_add(activated.token_cost);
            usage.total_items = usage.total_items.saturating_add(1);
        }
        let charge = activated
            .token_cost
            .saturating_mul(u64::try_from(activated.deliveries.len()).unwrap_or(u64::MAX));
        match metadata.rule.budget_class {
            ActivationBudgetClass::Normal => usage.normal_tokens = usage.normal_tokens.saturating_add(charge),
            ActivationBudgetClass::Reserved => usage.reserved_tokens = usage.reserved_tokens.saturating_add(charge),
            ActivationBudgetClass::Mandatory => usage.mandatory_tokens = usage.mandatory_tokens.saturating_add(charge),
        }
    }
    Ok(usage)
}

fn admit_deliveries(
    request: &ActivationRequest<'_>,
    budget_class: ActivationBudgetClass,
    forced_mandatory: bool,
    token_cost: u64,
    deliveries: &[KnowledgeDelivery],
    usage: &mut BudgetUsage,
) -> AdmissionOutcome {
    let additional_items = deliveries.len();
    let additional_tokens = token_cost.saturating_mul(u64::try_from(additional_items).unwrap_or(u64::MAX));
    if usage.total_items.saturating_add(additional_items) > request.limits.max_total_items {
        return AdmissionOutcome::Rejected;
    }
    for delivery in deliveries {
        if usage
            .audience_items
            .get(delivery)
            .copied()
            .unwrap_or_default()
            .saturating_add(1)
            > request.limits.max_items_per_audience
            || usage
                .audience_tokens
                .get(delivery)
                .copied()
                .unwrap_or_default()
                .saturating_add(token_cost)
                > request.limits.max_tokens_per_audience
        {
            return AdmissionOutcome::Rejected;
        }
    }
    let total_tokens = usage
        .normal_tokens
        .saturating_add(usage.reserved_tokens)
        .saturating_add(usage.mandatory_tokens);
    if total_tokens.saturating_add(additional_tokens) > request.limits.max_total_tokens {
        return AdmissionOutcome::Rejected;
    }
    let effective_class = if forced_mandatory {
        ActivationBudgetClass::Mandatory
    } else {
        budget_class
    };
    let normal_ceiling = request
        .limits
        .max_total_tokens
        .saturating_sub(request.limits.reserved_tokens)
        .saturating_sub(request.limits.mandatory_tokens);
    let reserved_ceiling = request.limits.max_total_tokens.saturating_sub(request.limits.mandatory_tokens);
    match effective_class {
        ActivationBudgetClass::Normal if usage.normal_tokens.saturating_add(additional_tokens) > normal_ceiling => {
            return AdmissionOutcome::Rejected;
        }
        ActivationBudgetClass::Reserved
            if usage
                .normal_tokens
                .saturating_add(usage.reserved_tokens)
                .saturating_add(additional_tokens)
                > reserved_ceiling =>
        {
            return AdmissionOutcome::Rejected;
        }
        _ => {}
    }
    for delivery in deliveries {
        *usage.audience_items.entry(delivery.clone()).or_default() += 1;
        *usage.audience_tokens.entry(delivery.clone()).or_default() = usage
            .audience_tokens
            .get(delivery)
            .copied()
            .unwrap_or_default()
            .saturating_add(token_cost);
    }
    usage.total_items = usage.total_items.saturating_add(additional_items);
    match effective_class {
        ActivationBudgetClass::Normal => usage.normal_tokens = usage.normal_tokens.saturating_add(additional_tokens),
        ActivationBudgetClass::Reserved => {
            usage.reserved_tokens = usage.reserved_tokens.saturating_add(additional_tokens);
        }
        ActivationBudgetClass::Mandatory => {
            usage.mandatory_tokens = usage.mandatory_tokens.saturating_add(additional_tokens);
        }
    }
    AdmissionOutcome::Admitted
}

fn validate_deliveries(kind: KnowledgeKind, deliveries: &[KnowledgeDelivery]) -> Result<(), ActivationError> {
    if deliveries.iter().any(|delivery| match delivery {
        KnowledgeDelivery::Writer => !matches!(kind, KnowledgeKind::Fact | KnowledgeKind::Rumor),
        KnowledgeDelivery::Character { .. } => kind != KnowledgeKind::Rumor,
    }) {
        return Err(ActivationError::ExternalTargetUnauthorized);
    }
    Ok(())
}

fn is_mandatory(class: ActivationBudgetClass, forced: bool) -> bool {
    forced || class == ActivationBudgetClass::Mandatory
}

fn update_timed_state(
    request: &ActivationRequest<'_>,
    metadata: &ActivationEntryMetadata,
    delta: &mut PendingActivationStateDelta,
) {
    let sticky_turns = u64::from(metadata.rule.timing.sticky_turns);
    let cooldown_turns = u64::from(metadata.rule.timing.cooldown_turns);
    if sticky_turns == 0 && cooldown_turns == 0 {
        return;
    }
    let sticky_through_turn = if sticky_turns == 0 {
        None
    } else {
        turn_number(request.turn_number.get().saturating_add(sticky_turns))
    };
    let cooldown_through_turn = if cooldown_turns == 0 {
        None
    } else {
        turn_number(
            request
                .turn_number
                .get()
                .saturating_add(sticky_turns)
                .saturating_add(cooldown_turns),
        )
    };
    delta.deletes.remove(&metadata.source_id);
    delta.upserts.retain(|state| state.source_id != metadata.source_id);
    delta.upserts.push(ActivationTimedState {
        source_id: metadata.source_id.clone(),
        rule_version: metadata.rule_version.clone(),
        sticky_through_turn,
        cooldown_through_turn,
    });
}

fn cleanup_timed_state(
    request: &ActivationRequest<'_>,
    continuation: &ActivationContinuation,
    timed: &BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    delta: &mut PendingActivationStateDelta,
) {
    for (source_id, state) in timed {
        let expired = state.sticky_through_turn.is_none_or(|through| through < request.turn_number)
            && state.cooldown_through_turn.is_none_or(|through| through < request.turn_number);
        if expired && !continuation.activated.contains_key(source_id) {
            delta.deletes.insert(source_id.clone());
        }
    }
    delta.upserts.sort_by(|left, right| left.source_id.cmp(&right.source_id));
}

fn ranked_activated(continuation: &mut ActivationContinuation) -> Vec<ActivatedKnowledgeRef> {
    let mut activated = continuation.activated.values().cloned().collect::<Vec<_>>();
    activated.sort_by(|left, right| {
        left.ordering
            .cmp(&right.ordering)
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
    for (index, entry) in activated.iter_mut().enumerate() {
        entry.rank = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
        if let Some(stored) = continuation.activated.get_mut(&entry.source_id) {
            stored.rank = entry.rank;
        }
    }
    activated
}

fn reject(
    continuation: &mut ActivationContinuation,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
    source_id: KnowledgeSourceId,
    reason: ActivationRejectionReason,
) {
    if continuation.terminal_rejections.insert(source_id, reason).is_none() {
        increment(rejections, reason);
    }
}

fn increment(rejections: &mut BTreeMap<ActivationRejectionReason, u32>, reason: ActivationRejectionReason) {
    *rejections.entry(reason).or_default() += 1;
}

fn bytes_to_tokens(bytes: usize) -> u64 {
    u64::try_from(bytes.div_ceil(4)).unwrap_or(u64::MAX)
}

fn turn_number(value: u64) -> Option<TurnNumber> {
    TurnNumber::try_new(value).ok()
}

fn source_priority(kind: ScanFragmentKind) -> u8 {
    match kind {
        ScanFragmentKind::PlayerContribution => 0,
        ScanFragmentKind::PlayerRoleName | ScanFragmentKind::PlayerRoleLabel => 1,
        ScanFragmentKind::NarrativeDirection | ScanFragmentKind::NarrativeEvent => 2,
        ScanFragmentKind::RecentStory => 3,
        ScanFragmentKind::StorySummary => 4,
        ScanFragmentKind::RecursionContent => 5,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationStoreFailure {
    Unavailable,
    RevisionConflict,
    NotFound,
    LimitExceeded,
    Serialization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationError {
    #[error("activation rule is invalid: {code}")]
    InvalidRule { code: &'static str },
    #[error("activation regex is invalid or unsupported")]
    InvalidRegex,
    #[error("activation index version mismatch")]
    IndexVersionMismatch,
    #[error("activation snapshot mismatch")]
    SnapshotMismatch,
    #[error("activation continuation mismatch")]
    ContinuationMismatch,
    #[error("activation work limit exceeded: {limit}")]
    WorkLimitExceeded { limit: &'static str },
    #[error("activation recursion step limit reached")]
    RecursionLimitReached,
    #[error("mandatory knowledge budget exceeded")]
    MandatoryBudgetExceeded,
    #[error("external activation target is unauthorized")]
    ExternalTargetUnauthorized,
    #[error("activation timed state is inconsistent")]
    TimedStateInconsistent,
    #[error("activation seed provider failed: {provider}")]
    ProviderFailure { provider: &'static str },
    #[error("activation store operation failed")]
    Store(ActivationStoreFailure),
}

impl ActivationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRule { .. } => "activation_rule_invalid",
            Self::InvalidRegex => "activation_regex_invalid",
            Self::IndexVersionMismatch => "activation_index_mismatch",
            Self::SnapshotMismatch => "activation_snapshot_conflict",
            Self::ContinuationMismatch => "activation_continuation_mismatch",
            Self::WorkLimitExceeded { .. } => "activation_work_limit",
            Self::RecursionLimitReached => "activation_recursion_limit",
            Self::MandatoryBudgetExceeded => "activation_mandatory_budget",
            Self::ExternalTargetUnauthorized => "activation_target_unauthorized",
            Self::TimedStateInconsistent => "activation_timed_state_invalid",
            Self::ProviderFailure { .. } => "activation_provider_failed",
            Self::Store(_) => "store_unavailable",
        }
    }
}

#[cfg(test)]
#[path = "tests/engine_tests.rs"]
mod tests;
