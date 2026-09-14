use super::contracts::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEvidence, ActivationExecutionInput, ActivationOrdering,
    ActivationPatternKind, ActivationRejectionReason, ActivationRequest, ActivationResult, ActivationWorkUsage,
};
use super::rule::{
    ActivationBudgetClass, ActivationGroupKey, ActivationPattern, SecondaryLogic, normalize_activation_literal,
};
use super::scan::{ScanFragment, ScanFragmentKind};
use super::state::{ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta};
use crate::domain::ids::TurnNumber;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::turn::KnowledgeDelivery;
use regex::RegexBuilder;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub struct KnowledgeActivationEngine;

const MATCHER_VERSION: u32 = 1;

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

struct RoundOutcome {
    newly_activated: Vec<KnowledgeSourceId>,
    budget_trimmed: bool,
}

struct TimedView {
    valid: BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    delta: PendingActivationStateDelta,
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

type PatternMatchResult = (usize, BTreeSet<u16>, Vec<ActivationEvidence>);

impl KnowledgeActivationEngine {
    pub fn run(&self, request: ActivationRequest<'_>) -> Result<ActivationResult, ActivationError> {
        validate_request(&request)?;
        let input = request
            .index_snapshot
            .execution_input()
            .ok_or(ActivationError::MissingExecutionInput)?;
        let mut continuation = match request.continuation.as_ref() {
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
        let mut timed = timed_view(&request)?;
        let mut rejections = BTreeMap::new();
        let mut budget = budget_usage(&request, &continuation)?;
        account_scan_fragments(&request, &mut continuation)?;
        let mut recursion_fragments = Vec::new();
        let mut recursion_pending = Vec::new();
        let mut budget_trimmed = false;
        let mut recursion_exhausted = false;

        let initial = process_round(
            &request,
            input,
            &mut continuation,
            &timed.valid,
            &mut timed.delta,
            &mut rejections,
            &mut budget,
            &recursion_fragments,
        )?;
        recursion_pending.extend(initial.newly_activated);
        budget_trimmed |= initial.budget_trimmed;

        loop {
            let added = add_recursion_fragments(
                &request,
                input,
                &mut continuation,
                &mut recursion_fragments,
                &mut recursion_pending,
            )?;
            if added == 0 {
                break;
            }
            if continuation.recursion_level >= request.limits.max_recursion_steps {
                recursion_exhausted = true;
                break;
            }
            continuation.recursion_level = continuation.recursion_level.saturating_add(1);
            continuation.consumed.recursion_steps = continuation.consumed.recursion_steps.saturating_add(1);
            let outcome = process_round(
                &request,
                input,
                &mut continuation,
                &timed.valid,
                &mut timed.delta,
                &mut rejections,
                &mut budget,
                &recursion_fragments,
            )?;
            recursion_pending.extend(outcome.newly_activated);
            budget_trimmed |= outcome.budget_trimmed;
        }

        while request.limits.minimum_activations > continuation.activated.len()
            && continuation.scan_depth < request.limits.max_scan_depth
            && continuation.depth_expansions < request.limits.max_depth_expansions
        {
            continuation.scan_depth = continuation.scan_depth.saturating_add(1);
            continuation.depth_expansions = continuation.depth_expansions.saturating_add(1);
            account_scan_fragments(&request, &mut continuation)?;
            let outcome = process_round(
                &request,
                input,
                &mut continuation,
                &timed.valid,
                &mut timed.delta,
                &mut rejections,
                &mut budget,
                &recursion_fragments,
            )?;
            recursion_pending.extend(outcome.newly_activated);
            budget_trimmed |= outcome.budget_trimmed;
            loop {
                let added = add_recursion_fragments(
                    &request,
                    input,
                    &mut continuation,
                    &mut recursion_fragments,
                    &mut recursion_pending,
                )?;
                if added == 0 {
                    break;
                }
                if continuation.recursion_level >= request.limits.max_recursion_steps {
                    recursion_exhausted = true;
                    break;
                }
                continuation.recursion_level = continuation.recursion_level.saturating_add(1);
                continuation.consumed.recursion_steps = continuation.consumed.recursion_steps.saturating_add(1);
                let outcome = process_round(
                    &request,
                    input,
                    &mut continuation,
                    &timed.valid,
                    &mut timed.delta,
                    &mut rejections,
                    &mut budget,
                    &recursion_fragments,
                )?;
                recursion_pending.extend(outcome.newly_activated);
                budget_trimmed |= outcome.budget_trimmed;
            }
            if recursion_exhausted {
                break;
            }
        }

        cleanup_timed_state(&request, &continuation, &timed.valid, &mut timed.delta);
        continuation.consumed.knowledge_tokens = budget
            .normal_tokens
            .saturating_add(budget.reserved_tokens)
            .saturating_add(budget.mandatory_tokens);
        let activated = ranked_activated(&mut continuation);
        let stop_reason = if recursion_exhausted {
            ActivationStopReason::RecursionExhausted
        } else if budget_trimmed {
            ActivationStopReason::WorkTrimmed
        } else if request.limits.minimum_activations > activated.len()
            && (continuation.scan_depth == request.limits.max_scan_depth
                || continuation.depth_expansions == request.limits.max_depth_expansions)
        {
            ActivationStopReason::MaximumDepthReached
        } else if continuation.depth_expansions > 0 && activated.len() >= request.limits.minimum_activations {
            ActivationStopReason::MinimumSatisfied
        } else {
            ActivationStopReason::Complete
        };
        Ok(ActivationResult {
            activated,
            continuation,
            pending_timed_state: timed.delta,
            rejection_summary: rejections,
            stop_reason,
        })
    }
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
    let mut literals = 0usize;
    let mut regexes = 0usize;
    for metadata in request.index_snapshot.metadata.values() {
        metadata
            .rule
            .validate(metadata.rule.selection.groups.len())
            .map_err(|_| ActivationError::InvalidRule { code: "invalid_rule" })?;
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
        for pattern in metadata
            .rule
            .match_rule
            .keys
            .iter()
            .chain(metadata.rule.match_rule.secondary_keys.iter())
        {
            let ActivationPattern::Literal(value) = pattern;
            if regex_parts(value).is_some() {
                regexes = regexes.saturating_add(1);
            } else {
                literals = literals.saturating_add(1);
            }
        }
    }
    if literals > limits.max_literal_patterns {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "literal_patterns",
        });
    }
    if regexes > limits.max_regex_patterns {
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

fn account_scan_fragments(
    request: &ActivationRequest<'_>,
    continuation: &mut ActivationContinuation,
) -> Result<(), ActivationError> {
    for fragment in visible_base_fragments(request, continuation.scan_depth) {
        if !continuation.scanned_fragment_ids.insert(fragment.id.clone()) {
            continue;
        }
        continuation.consumed.scan_fragments = continuation.consumed.scan_fragments.saturating_add(1);
        continuation.consumed.scan_bytes =
            continuation.consumed.scan_bytes.saturating_add(fragment.text.as_str().len());
        continuation.consumed.scan_tokens = continuation
            .consumed
            .scan_tokens
            .saturating_add(bytes_to_tokens(fragment.text.as_str().len()));
    }
    if continuation.consumed.scan_fragments > request.limits.max_scan_fragments {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "scan_fragments",
        });
    }
    if continuation.consumed.scan_bytes > request.limits.max_scan_bytes {
        return Err(ActivationError::WorkLimitExceeded { limit: "scan_bytes" });
    }
    if continuation.consumed.scan_tokens > request.limits.max_scan_tokens {
        return Err(ActivationError::WorkLimitExceeded { limit: "scan_tokens" });
    }
    Ok(())
}

fn process_round(
    request: &ActivationRequest<'_>,
    input: &ActivationExecutionInput,
    continuation: &mut ActivationContinuation,
    timed: &BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    timed_delta: &mut PendingActivationStateDelta,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
    budget: &mut BudgetUsage,
    recursion_fragments: &[ScanFragment],
) -> Result<RoundOutcome, ActivationError> {
    let mut candidates = collect_candidates(request, input, continuation, timed, rejections, recursion_fragments)?;
    if candidates.len() > request.limits.max_candidates_per_round {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "candidates_per_round",
        });
    }
    let mut eligible = Vec::new();
    for candidate in candidates.values_mut() {
        continuation.consumed.candidate_evaluations = continuation.consumed.candidate_evaluations.saturating_add(1);
        let metadata = request
            .index_snapshot
            .metadata
            .get(&candidate.source_id)
            .ok_or(ActivationError::ExternalTargetUnauthorized)?;
        if let Some(existing) = continuation.activated.get_mut(&candidate.source_id) {
            let new_deliveries = candidate
                .deliveries
                .iter()
                .filter(|delivery| !existing.deliveries.contains(delivery))
                .cloned()
                .collect::<Vec<_>>();
            if new_deliveries.is_empty() {
                continue;
            }
            validate_deliveries(metadata.kind, &new_deliveries)?;
            let entry_input = input.entry(&candidate.source_id).ok_or(ActivationError::MissingEntryInput)?;
            admit_deliveries(
                request,
                metadata.rule.budget_class,
                candidate.mandatory,
                entry_input.token_cost,
                &new_deliveries,
                budget,
            )?;
            existing.deliveries.extend(new_deliveries);
            existing.deliveries.sort();
            continue;
        }
        if continuation.terminal_rejections.contains_key(&candidate.source_id) {
            continue;
        }
        if !metadata.rule.mode.enabled {
            reject(
                continuation,
                rejections,
                candidate.source_id.clone(),
                ActivationRejectionReason::Disabled,
            );
            continue;
        }
        if !metadata.rule.scope.generation_triggers.is_empty()
            && !metadata.rule.scope.generation_triggers.contains(&request.generation_trigger)
        {
            reject(
                continuation,
                rejections,
                candidate.source_id.clone(),
                ActivationRejectionReason::ScopeMismatch,
            );
            continue;
        }
        if metadata.rule.timing.delay_turns > 0
            && request.turn_number.get() <= u64::from(metadata.rule.timing.delay_turns)
        {
            reject(
                continuation,
                rejections,
                candidate.source_id.clone(),
                ActivationRejectionReason::Delayed,
            );
            continue;
        }
        if !candidate.sticky
            && timed.get(&candidate.source_id).is_some_and(|state| {
                state
                    .cooldown_through_turn
                    .is_some_and(|through| through >= request.turn_number)
            })
        {
            reject(
                continuation,
                rejections,
                candidate.source_id.clone(),
                ActivationRejectionReason::Cooldown,
            );
            continue;
        }
        if !candidate.sticky && candidate.recursion_only && metadata.rule.recursion.exclude_recursion {
            reject(
                continuation,
                rejections,
                candidate.source_id.clone(),
                ActivationRejectionReason::RecursionExcluded,
            );
            continue;
        }
        if !candidate.sticky
            && metadata
                .rule
                .recursion
                .delay_until_recursion
                .is_some_and(|level| continuation.recursion_level < level)
        {
            increment(rejections, ActivationRejectionReason::RecursionLevelLocked);
            continue;
        }
        eligible.push(candidate.clone());
    }

    eligible = resolve_groups(request, continuation, rejections, eligible);
    eligible.sort_by(|left, right| candidate_order(request, left, right));
    let mut newly_activated = Vec::new();
    let mut budget_trimmed = false;
    for candidate in eligible {
        let metadata = request
            .index_snapshot
            .metadata
            .get(&candidate.source_id)
            .ok_or(ActivationError::ExternalTargetUnauthorized)?;
        if !candidate.sticky
            && !probability_admits(
                request.story_id.as_str(),
                request.turn_number.get(),
                &candidate.source_id,
                metadata.rule.selection.probability,
                metadata.rule_version.0.as_bytes(),
            )
        {
            continuation.failed_probability.insert(candidate.source_id.clone());
            reject(
                continuation,
                rejections,
                candidate.source_id,
                ActivationRejectionReason::Probability,
            );
            continue;
        }
        let entry_input = input.entry(&candidate.source_id).ok_or(ActivationError::MissingEntryInput)?;
        if entry_input.body.as_str().len() > request.limits.max_single_entry_bytes {
            if is_mandatory(metadata.rule.budget_class, candidate.mandatory) {
                return Err(ActivationError::MandatoryBudgetExceeded);
            }
            reject(continuation, rejections, candidate.source_id, ActivationRejectionReason::Budget);
            budget_trimmed = true;
            continue;
        }
        let mut deliveries = entry_input.deliveries.clone();
        deliveries.extend(candidate.deliveries.clone());
        deliveries.sort();
        deliveries.dedup();
        validate_deliveries(metadata.kind, &deliveries)?;
        if continuation.activated.len() >= request.limits.max_activated_entries {
            if is_mandatory(metadata.rule.budget_class, candidate.mandatory) {
                return Err(ActivationError::MandatoryBudgetExceeded);
            }
            reject(continuation, rejections, candidate.source_id, ActivationRejectionReason::Budget);
            budget_trimmed = true;
            continue;
        }
        if let Err(error) = admit_deliveries(
            request,
            metadata.rule.budget_class,
            candidate.mandatory,
            entry_input.token_cost,
            &deliveries,
            budget,
        ) {
            if is_mandatory(metadata.rule.budget_class, candidate.mandatory) {
                return Err(error);
            }
            reject(continuation, rejections, candidate.source_id, ActivationRejectionReason::Budget);
            budget_trimmed = true;
            continue;
        }
        let ordering = activation_ordering(request, &candidate);
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
        evidence.truncate(request.limits.max_evidence_per_entry);
        let evidence_bytes = evidence.len().saturating_mul(std::mem::size_of::<ActivationEvidence>());
        if continuation.evidence_bytes.saturating_add(evidence_bytes) > request.limits.max_evidence_bytes {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "evidence_bytes",
            });
        }
        continuation.evidence_bytes = continuation.evidence_bytes.saturating_add(evidence_bytes);
        let source_id = candidate.source_id.clone();
        continuation.activated.insert(
            source_id.clone(),
            ActivatedKnowledgeRef {
                source_id: source_id.clone(),
                deliveries,
                activation_class: candidate.class,
                rank: 0,
                token_cost: entry_input.token_cost,
                evidence,
                ordering,
            },
        );
        continuation.consumed.activated_entries = continuation.consumed.activated_entries.saturating_add(1);
        if !candidate.sticky {
            update_timed_state(request, metadata, timed_delta);
        }
        newly_activated.push(source_id);
    }
    Ok(RoundOutcome {
        newly_activated,
        budget_trimmed,
    })
}

fn collect_candidates(
    request: &ActivationRequest<'_>,
    input: &ActivationExecutionInput,
    continuation: &mut ActivationContinuation,
    timed: &BTreeMap<KnowledgeSourceId, ActivationTimedState>,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
    recursion_fragments: &[ScanFragment],
) -> Result<BTreeMap<KnowledgeSourceId, Candidate>, ActivationError> {
    let mut candidates = BTreeMap::new();
    for source_id in &request.index_snapshot.constant_entries {
        let metadata = request
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
    for (source_id, state) in timed {
        if state.sticky_through_turn.is_some_and(|through| through >= request.turn_number) {
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
    for seed in request.external_seeds {
        if !request.index_snapshot.metadata.contains_key(&seed.source_id) {
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
    let round = continuation.depth_expansions.saturating_add(continuation.recursion_level);
    for (source_id, metadata) in &request.index_snapshot.metadata {
        if !metadata.rule.mode.enabled || metadata.rule.mode.constant || metadata.rule.mode.exact_target_only {
            continue;
        }
        let entry_depth = metadata
            .rule
            .match_rule
            .scan_depth
            .unwrap_or(continuation.scan_depth)
            .min(continuation.scan_depth);
        let mut fragments = visible_base_fragments(request, entry_depth);
        fragments.extend(recursion_fragments.iter());
        let primary = collect_pattern_matches(
            &metadata.rule.match_rule.keys,
            &fragments,
            metadata.rule.match_rule.case_sensitive,
            metadata.rule.match_rule.match_whole_words,
            ActivationPatternKind::PrimaryLiteral,
            input,
            round,
            continuation.recursion_level,
            true,
        )?;
        let secondary_positive = matches!(
            metadata.rule.match_rule.secondary_logic,
            SecondaryLogic::AndAny | SecondaryLogic::AndAll
        );
        let secondary = collect_pattern_matches(
            &metadata.rule.match_rule.secondary_keys,
            &fragments,
            metadata.rule.match_rule.case_sensitive,
            metadata.rule.match_rule.match_whole_words,
            ActivationPatternKind::SecondaryLiteral,
            input,
            round,
            continuation.recursion_level,
            secondary_positive,
        )?;
        continuation.consumed.pattern_matches = continuation
            .consumed
            .pattern_matches
            .saturating_add(primary.0)
            .saturating_add(secondary.0);
        if continuation.consumed.pattern_matches > request.limits.max_pattern_matches {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "pattern_matches",
            });
        }
        if primary.1.is_empty() {
            continue;
        }
        if !secondary_logic(
            metadata.rule.match_rule.secondary_logic,
            &secondary.1,
            metadata.rule.match_rule.secondary_keys.len(),
        ) {
            increment(rejections, ActivationRejectionReason::SecondaryCondition);
            continue;
        }
        let mut evidence = primary.2;
        evidence.extend(secondary.2);
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
            &mut candidates,
            Candidate {
                source_id: source_id.clone(),
                class: if recursion_only {
                    ActivationSeedKind::Recursion
                } else {
                    ActivationSeedKind::TextMatch
                },
                score: u16::try_from(primary.1.len().saturating_add(secondary.1.len())).unwrap_or(u16::MAX),
                evidence,
                provider_rank: None,
                mandatory: metadata.rule.budget_class == ActivationBudgetClass::Mandatory,
                sticky: false,
                recursion_only,
                deliveries: Vec::new(),
            },
        );
    }
    Ok(candidates)
}

fn collect_pattern_matches(
    patterns: &[ActivationPattern],
    fragments: &[&ScanFragment],
    case_sensitive: bool,
    whole_words: bool,
    kind: ActivationPatternKind,
    input: &ActivationExecutionInput,
    round: u16,
    recursion_level: u16,
    positive_score: bool,
) -> Result<PatternMatchResult, ActivationError> {
    let mut count = 0usize;
    let mut matched = BTreeSet::new();
    let mut evidence = Vec::new();
    for (ordinal, pattern) in patterns.iter().enumerate() {
        let ActivationPattern::Literal(value) = pattern;
        let (regex, is_regex) = parse_regex(value)?;
        let literal = if is_regex {
            None
        } else {
            Some(expand_macros(value, input)?)
        };
        for fragment in fragments {
            let match_count = if let Some(regex) = &regex {
                regex.find_iter(fragment.text.as_str()).count()
            } else {
                literal_count(
                    fragment.text.as_str(),
                    literal.as_deref().unwrap_or_default(),
                    case_sensitive,
                    whole_words,
                )
            };
            if match_count == 0 {
                continue;
            }
            count = count.saturating_add(match_count);
            let ordinal = u16::try_from(ordinal).unwrap_or(u16::MAX);
            matched.insert(ordinal);
            evidence.push(ActivationEvidence {
                pattern_kind: if is_regex {
                    match kind {
                        ActivationPatternKind::PrimaryLiteral => ActivationPatternKind::PrimaryRegex,
                        ActivationPatternKind::SecondaryLiteral => ActivationPatternKind::SecondaryRegex,
                        other => other,
                    }
                } else {
                    kind
                },
                pattern_ordinal: ordinal,
                fragment_kind: fragment.kind,
                recency_depth: fragment.recency_depth,
                stable_fragment_order: fragment.stable_order,
                match_count: u16::try_from(match_count).unwrap_or(u16::MAX),
                group_score_contribution: u16::from(positive_score),
                round,
                recursion_level,
            });
        }
    }
    Ok((count, matched, evidence))
}

fn visible_base_fragments<'a>(request: &'a ActivationRequest<'_>, depth: u16) -> Vec<&'a ScanFragment> {
    request
        .scan_buffer
        .visible_at_depth(depth)
        .filter(|fragment| {
            fragment.kind != ScanFragmentKind::RecursionContent
                && (fragment.kind != ScanFragmentKind::StorySummary
                    || (request.limits.include_summary_at_max_depth && depth == request.limits.max_scan_depth))
        })
        .collect()
}

fn expand_macros(value: &str, input: &ActivationExecutionInput) -> Result<String, ActivationError> {
    let expanded = value
        .replace("{{player_name}}", &input.macros().player_name)
        .replace("{{player_role_label}}", &input.macros().player_role_label);
    if expanded.len() > input.max_macro_expansion_bytes() {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "macro_expansion_bytes",
        });
    }
    Ok(expanded)
}

fn parse_regex(value: &str) -> Result<(Option<regex::Regex>, bool), ActivationError> {
    let Some((expression, flags)) = regex_parts(value) else {
        return Ok((None, false));
    };
    if flags.chars().any(|flag| !matches!(flag, 'i' | 'm' | 's' | 'u'))
        || flags.chars().collect::<BTreeSet<_>>().len() != flags.chars().count()
    {
        return Err(ActivationError::InvalidRegex);
    }
    let regex = RegexBuilder::new(expression)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .unicode(true)
        .build()
        .map_err(|_| ActivationError::InvalidRegex)?;
    Ok((Some(regex), true))
}

fn regex_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('/')?;
    let end = rest.rfind('/')?;
    Some((&rest[..end], &rest[end + 1..]))
}

fn literal_count(text: &str, pattern: &str, case_sensitive: bool, whole_words: bool) -> usize {
    let text = normalize_activation_literal(text, case_sensitive);
    let pattern = normalize_activation_literal(pattern, case_sensitive);
    if pattern.is_empty() {
        return 0;
    }
    text.match_indices(&pattern)
        .filter(|(offset, value)| {
            !whole_words || {
                let before = text[..*offset].chars().next_back();
                let after = text[*offset + value.len()..].chars().next();
                !before.is_some_and(|item| item.is_alphanumeric() || item == '_')
                    && !after.is_some_and(|item| item.is_alphanumeric() || item == '_')
            }
        })
        .count()
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
        hasher.update(metadata.rule_version.0.as_bytes());
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
) -> Result<(), ActivationError> {
    let additional_items = deliveries.len();
    let additional_tokens = token_cost.saturating_mul(u64::try_from(additional_items).unwrap_or(u64::MAX));
    if usage.total_items.saturating_add(additional_items) > request.limits.max_total_items {
        return Err(ActivationError::MandatoryBudgetExceeded);
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
            return Err(ActivationError::MandatoryBudgetExceeded);
        }
    }
    let total_tokens = usage
        .normal_tokens
        .saturating_add(usage.reserved_tokens)
        .saturating_add(usage.mandatory_tokens);
    if total_tokens.saturating_add(additional_tokens) > request.limits.max_total_tokens {
        return Err(ActivationError::MandatoryBudgetExceeded);
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
            return Err(ActivationError::MandatoryBudgetExceeded);
        }
        ActivationBudgetClass::Reserved
            if usage
                .normal_tokens
                .saturating_add(usage.reserved_tokens)
                .saturating_add(additional_tokens)
                > reserved_ceiling =>
        {
            return Err(ActivationError::MandatoryBudgetExceeded);
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
    Ok(())
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

fn add_recursion_fragments(
    request: &ActivationRequest<'_>,
    input: &ActivationExecutionInput,
    continuation: &mut ActivationContinuation,
    recursion_fragments: &mut Vec<ScanFragment>,
    pending: &mut Vec<KnowledgeSourceId>,
) -> Result<usize, ActivationError> {
    pending.sort();
    pending.dedup();
    let mut added = 0usize;
    for source_id in std::mem::take(pending) {
        if continuation.recursion_sources.contains(&source_id) {
            continue;
        }
        let metadata = request
            .index_snapshot
            .metadata
            .get(&source_id)
            .ok_or(ActivationError::IndexVersionMismatch)?;
        continuation.recursion_sources.insert(source_id.clone());
        if metadata.rule.recursion.prevent_recursion {
            continue;
        }
        let entry_input = input.entry(&source_id).ok_or(ActivationError::MissingEntryInput)?;
        let bytes = entry_input.body.as_str().len();
        if continuation.consumed.recursion_fragments.saturating_add(1) > request.limits.max_recursion_fragments
            || continuation.consumed.recursion_bytes.saturating_add(bytes) > request.limits.max_recursion_bytes
            || continuation.consumed.recursion_tokens.saturating_add(entry_input.token_cost)
                > request.limits.max_recursion_tokens
        {
            return Err(ActivationError::RecursionLimitReached);
        }
        let stable_order = u32::try_from(recursion_fragments.len()).unwrap_or(u32::MAX);
        recursion_fragments.push(ScanFragment::new(
            ScanFragmentKind::RecursionContent,
            continuation.recursion_level.saturating_add(1),
            stable_order,
            entry_input.body.clone(),
        ));
        continuation.consumed.recursion_fragments = continuation.consumed.recursion_fragments.saturating_add(1);
        continuation.consumed.recursion_bytes = continuation.consumed.recursion_bytes.saturating_add(bytes);
        continuation.consumed.recursion_tokens =
            continuation.consumed.recursion_tokens.saturating_add(entry_input.token_cost);
        added = added.saturating_add(1);
    }
    Ok(added)
}

fn update_timed_state(
    request: &ActivationRequest<'_>,
    metadata: &super::contracts::ActivationEntryMetadata,
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
    #[error("activation execution input is missing")]
    MissingExecutionInput,
    #[error("activation execution input entry is missing")]
    MissingEntryInput,
}
