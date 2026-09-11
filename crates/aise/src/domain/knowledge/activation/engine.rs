use super::contracts::{
    ActivatedKnowledgeRef, ActivationContinuation, ActivationEvidence, ActivationMachineState, ActivationPatternKind,
    ActivationRejectionReason, ActivationRequest, ActivationResult, ActivationWorkUsage,
};
use super::rule::{ActivationPattern, SecondaryLogic, normalize_activation_literal};
use super::state::{ActivationSeedKind, ActivationStopReason, ActivationTimedState, PendingActivationStateDelta};
use crate::domain::knowledge::KnowledgeSourceId;
use regex::RegexBuilder;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub struct KnowledgeActivationEngine;

const MATCHER_VERSION: u32 = 1;

struct Candidate {
    source_id: KnowledgeSourceId,
    class: ActivationSeedKind,
    evidence: Vec<ActivationEvidence>,
    score: u16,
    provider_rank: Option<u32>,
    mandatory: bool,
    sticky: bool,
}

type PatternMatchResult = (usize, BTreeMap<u16, bool>, Vec<ActivationEvidence>);

impl KnowledgeActivationEngine {
    pub fn run(&self, request: ActivationRequest<'_>) -> Result<ActivationResult, ActivationError> {
        validate_limits(&request)?;
        if !request
            .index_snapshot
            .matches_snapshot(request.knowledge_snapshot, MATCHER_VERSION)
        {
            return Err(ActivationError::SnapshotMismatch);
        }
        let resumed = request.continuation.is_some();
        let mut continuation = match request.continuation.as_ref() {
            Some(value) => {
                if value.turn_number != request.turn_number
                    || value.generation_trigger != request.generation_trigger
                    || value.knowledge_snapshot != *request.knowledge_snapshot
                    || value.index_snapshot != request.index_snapshot.reference
                {
                    return Err(ActivationError::ContinuationMismatch);
                }
                value.clone()
            }
            None => new_continuation(&request),
        };
        let mut rejections = BTreeMap::new();
        let mut usage = continuation.consumed;
        usage.scan_fragments = usage.scan_fragments.checked_add(request.scan_buffer.fragments().len()).ok_or(
            ActivationError::WorkLimitExceeded {
                limit: "scan_fragments",
            },
        )?;
        usage.scan_bytes = usage
            .scan_bytes
            .checked_add(
                request
                    .scan_buffer
                    .fragments()
                    .iter()
                    .map(|item| item.text.as_str().len())
                    .sum(),
            )
            .ok_or(ActivationError::WorkLimitExceeded { limit: "scan_bytes" })?;
        usage.scan_tokens = u64::try_from(usage.scan_bytes.div_ceil(4)).unwrap_or(u64::MAX);
        if usage.scan_fragments > request.limits.max_scan_fragments {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "scan_fragments",
            });
        }
        if usage.scan_bytes > request.limits.max_scan_bytes {
            return Err(ActivationError::WorkLimitExceeded { limit: "scan_bytes" });
        }
        if usage.scan_tokens > request.limits.max_scan_tokens {
            return Err(ActivationError::WorkLimitExceeded { limit: "scan_tokens" });
        }

        let state = if resumed {
            ActivationMachineState::Resumed
        } else {
            ActivationMachineState::Initial
        };
        let depth = request.limits.initial_scan_depth.min(request.limits.max_scan_depth);
        let mut candidates = collect_candidates(&request, depth, &mut usage, &mut rejections)?;
        let evidence_bytes = candidates
            .iter()
            .flat_map(|candidate| candidate.evidence.iter())
            .count()
            .saturating_mul(std::mem::size_of::<ActivationEvidence>());
        if evidence_bytes > request.limits.max_evidence_bytes {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "evidence_bytes",
            });
        }
        candidates.sort_by(candidate_order(&request, &continuation));
        if candidates.len() > request.limits.max_candidates_per_round {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "candidates_per_round",
            });
        }

        for candidate in candidates {
            usage.candidate_evaluations =
                usage
                    .candidate_evaluations
                    .checked_add(1)
                    .ok_or(ActivationError::WorkLimitExceeded {
                        limit: "candidate_evaluations",
                    })?;
            if continuation.activated.contains_key(&candidate.source_id)
                || continuation.terminal_rejections.contains_key(&candidate.source_id)
            {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Duplicate,
                );
                continue;
            }
            let entry = request
                .index_snapshot
                .metadata
                .get(&candidate.source_id)
                .ok_or(ActivationError::ExternalTargetUnauthorized)?;
            if !entry.rule.mode.enabled && !candidate.sticky {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Disabled,
                );
                continue;
            }
            if !entry.rule.scope.generation_triggers.is_empty()
                && !entry.rule.scope.generation_triggers.contains(&request.generation_trigger)
            {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::ScopeMismatch,
                );
                continue;
            }
            if entry.rule.timing.delay_turns > 0
                && request.turn_number.get() <= u64::from(entry.rule.timing.delay_turns)
            {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Delayed,
                );
                continue;
            }
            if !candidate.sticky
                && timed_state_is_cooling(&candidate.source_id, request.timed_state, request.turn_number)
            {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Cooldown,
                );
                continue;
            }
            if !candidate.sticky
                && !probability_admits(
                    request.story_id.as_str(),
                    request.turn_number.get(),
                    &candidate.source_id,
                    &entry.rule.selection.probability,
                    entry.rule_version.0.as_bytes(),
                )
            {
                continuation.failed_probability.insert(candidate.source_id.clone());
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Probability,
                );
                continue;
            }
            if usage.activated_entries >= request.limits.max_activated_entries
                || continuation.activated.len() >= request.limits.max_total_items
            {
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Budget,
                );
                continue;
            }
            let deliveries = request
                .external_seeds
                .iter()
                .filter(|seed| seed.source_id == candidate.source_id)
                .map(|seed| seed.delivery.clone())
                .collect();
            let token_cost = 0;
            if usage.knowledge_tokens.saturating_add(token_cost) > request.limits.max_total_tokens {
                if candidate.mandatory {
                    return Err(ActivationError::MandatoryBudgetExceeded);
                }
                reject(
                    &mut continuation,
                    &mut rejections,
                    candidate.source_id,
                    ActivationRejectionReason::Budget,
                );
                continue;
            }
            let activated = ActivatedKnowledgeRef {
                source_id: candidate.source_id.clone(),
                deliveries,
                activation_class: candidate.class,
                rank: 0,
                token_cost,
                evidence: candidate
                    .evidence
                    .into_iter()
                    .take(request.limits.max_evidence_per_entry)
                    .collect(),
            };
            usage.knowledge_tokens = usage.knowledge_tokens.saturating_add(token_cost);
            usage.activated_entries = usage.activated_entries.saturating_add(1);
            continuation.activated.insert(candidate.source_id, activated);
            if entry.rule.timing.sticky_turns > 0 || entry.rule.timing.cooldown_turns > 0 {
                let _ = &request.mode;
            }
        }
        continuation.consumed = usage;
        let mut activated = continuation.activated.values().cloned().collect::<Vec<_>>();
        activated.sort_by(|left, right| left.source_id.cmp(&right.source_id));
        for (rank, entry) in activated.iter_mut().enumerate() {
            entry.rank = u32::try_from(rank + 1).unwrap_or(u32::MAX);
            if let Some(stored) = continuation.activated.get_mut(&entry.source_id) {
                stored.rank = entry.rank;
            }
        }
        let stop_reason = if activated.len() >= request.limits.minimum_activations {
            ActivationStopReason::Complete
        } else if matches!(state, ActivationMachineState::Initial | ActivationMachineState::Resumed) {
            ActivationStopReason::MaximumDepthReached
        } else {
            ActivationStopReason::WorkTrimmed
        };
        Ok(ActivationResult {
            activated,
            continuation,
            pending_timed_state: timed_state_delta(&request, &rejections),
            rejection_summary: rejections,
            stop_reason,
        })
    }
}

fn validate_limits(request: &ActivationRequest<'_>) -> Result<(), ActivationError> {
    let limits = &request.limits;
    if limits.max_scan_fragments == 0
        || limits.max_scan_bytes == 0
        || limits.max_scan_tokens == 0
        || limits.max_scan_depth == 0
        || limits.max_literal_patterns == 0
        || limits.max_regex_patterns == 0
        || limits.max_pattern_matches == 0
        || limits.max_candidates_per_round == 0
        || limits.max_recursion_steps == 0
        || limits.max_recursion_fragments == 0
        || limits.max_recursion_bytes == 0
        || limits.max_recursion_tokens == 0
        || limits.max_activated_entries == 0
        || limits.max_depth_expansions == 0
        || limits.max_external_candidates == 0
        || limits.max_evidence_per_entry == 0
        || limits.max_evidence_bytes == 0
        || limits.max_items_per_audience == 0
        || limits.max_tokens_per_audience == 0
        || limits.max_total_items == 0
        || limits.max_total_tokens == 0
        || limits.max_single_entry_bytes == 0
        || limits.initial_scan_depth > limits.max_scan_depth
        || request.external_seeds.len() > limits.max_external_candidates
    {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "runtime_limits",
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
        failed_probability: Default::default(),
        group_winners: BTreeMap::new(),
        recursion_level: 0,
        consumed: ActivationWorkUsage::default(),
        evidence_bytes: 0,
    }
}

fn collect_candidates(
    request: &ActivationRequest<'_>,
    depth: u16,
    usage: &mut ActivationWorkUsage,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
) -> Result<Vec<Candidate>, ActivationError> {
    let mut candidates = Vec::new();
    for source_id in &request.index_snapshot.constant_entries {
        candidates.push(Candidate {
            source_id: source_id.clone(),
            class: ActivationSeedKind::Constant,
            evidence: Vec::new(),
            score: 0,
            provider_rank: None,
            mandatory: false,
            sticky: false,
        });
    }
    for seed in request.external_seeds {
        candidates.push(Candidate {
            source_id: seed.source_id.clone(),
            class: seed.kind,
            evidence: Vec::new(),
            score: 0,
            provider_rank: seed.provider_rank,
            mandatory: seed.mandatory,
            sticky: false,
        });
    }
    for (source_id, metadata) in &request.index_snapshot.metadata {
        metadata
            .rule
            .validate(metadata.rule.selection.groups.len())
            .map_err(|_| ActivationError::InvalidRule { code: "invalid_rule" })?;
        if !metadata.rule.mode.enabled || metadata.rule.mode.constant || metadata.rule.mode.exact_target_only {
            continue;
        }
        let visible = request.scan_buffer.visible_at_depth(depth);
        let primary = collect_pattern_matches(
            &metadata.rule.match_rule.keys,
            visible,
            metadata.rule.match_rule.case_sensitive,
            metadata.rule.match_rule.match_whole_words,
            ActivationPatternKind::PrimaryLiteral,
        )?;
        usage.pattern_matches = usage.pattern_matches.saturating_add(primary.0);
        if usage.pattern_matches > request.limits.max_pattern_matches {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "pattern_matches",
            });
        }
        let secondary = collect_pattern_matches(
            &metadata.rule.match_rule.secondary_keys,
            request.scan_buffer.visible_at_depth(depth),
            metadata.rule.match_rule.case_sensitive,
            metadata.rule.match_rule.match_whole_words,
            ActivationPatternKind::SecondaryLiteral,
        )?;
        usage.pattern_matches = usage.pattern_matches.saturating_add(secondary.0);
        if usage.pattern_matches > request.limits.max_pattern_matches {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "pattern_matches",
            });
        }
        let secondary_match_count = secondary.1.len();
        let secondary_ok = secondary_logic(
            metadata.rule.match_rule.secondary_logic,
            &secondary.1,
            metadata.rule.match_rule.secondary_keys.len(),
        );
        if !primary.1.is_empty() && secondary_ok {
            let mut evidence = primary.2;
            evidence.extend(secondary.2);
            candidates.push(Candidate {
                source_id: source_id.clone(),
                class: ActivationSeedKind::TextMatch,
                score: u16::try_from(primary.1.len() + secondary_match_count).unwrap_or(u16::MAX),
                evidence,
                provider_rank: None,
                mandatory: false,
                sticky: false,
            });
        } else if !primary.1.is_empty() {
            increment(rejections, ActivationRejectionReason::SecondaryCondition);
        }
    }
    Ok(candidates)
}

fn collect_pattern_matches<'a>(
    patterns: &[ActivationPattern],
    fragments: impl Iterator<Item = &'a super::scan::ScanFragment>,
    case_sensitive: bool,
    whole_words: bool,
    kind: ActivationPatternKind,
) -> Result<PatternMatchResult, ActivationError> {
    let fragments = fragments.collect::<Vec<_>>();
    let mut count: usize = 0;
    let mut matched = BTreeMap::new();
    let mut evidence = Vec::new();
    for (ordinal, pattern) in patterns.iter().enumerate() {
        let ActivationPattern::Literal(value) = pattern;
        let (regex, is_regex) = parse_regex(value)?;
        for fragment in &fragments {
            let match_count = if let Some(regex) = &regex {
                regex.find_iter(fragment.text.as_str()).count()
            } else {
                literal_count(fragment.text.as_str(), value, case_sensitive, whole_words)
            };
            if match_count > 0 {
                count = count.saturating_add(match_count);
                matched.insert(u16::try_from(ordinal).unwrap_or(u16::MAX), true);
                evidence.push(ActivationEvidence {
                    pattern_kind: if is_regex {
                        match kind {
                            ActivationPatternKind::PrimaryLiteral => ActivationPatternKind::PrimaryRegex,
                            ActivationPatternKind::SecondaryLiteral => ActivationPatternKind::SecondaryRegex,
                            value => value,
                        }
                    } else {
                        kind
                    },
                    pattern_ordinal: u16::try_from(ordinal).unwrap_or(u16::MAX),
                    fragment_kind: fragment.kind,
                    recency_depth: fragment.recency_depth,
                    stable_fragment_order: fragment.stable_order,
                    match_count: u16::try_from(match_count).unwrap_or(u16::MAX),
                    group_score_contribution: 1,
                    round: 0,
                    recursion_level: 0,
                });
            }
        }
    }
    Ok((count, matched, evidence))
}

fn parse_regex(value: &str) -> Result<(Option<regex::Regex>, bool), ActivationError> {
    let Some(rest) = value.strip_prefix('/') else {
        return Ok((None, false));
    };
    let Some(end) = rest.rfind('/') else {
        return Err(ActivationError::InvalidRegex);
    };
    let expression = &rest[..end];
    let flags = &rest[end + 1..];
    if flags.chars().any(|flag| !matches!(flag, 'i' | 'm' | 's' | 'u'))
        || flags.chars().collect::<std::collections::BTreeSet<_>>().len() != flags.chars().count()
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

fn secondary_logic(logic: SecondaryLogic, matches: &BTreeMap<u16, bool>, pattern_count: usize) -> bool {
    match logic {
        SecondaryLogic::AndAny => matches.values().any(|value| *value),
        SecondaryLogic::AndAll => matches.len() == pattern_count,
        SecondaryLogic::NotAny => matches.is_empty(),
        SecondaryLogic::NotAll => matches.len() < pattern_count,
    }
}

fn candidate_order<'a>(
    request: &'a ActivationRequest<'_>,
    continuation: &'a ActivationContinuation,
) -> impl FnMut(&Candidate, &Candidate) -> Ordering + 'a {
    move |left, right| {
        let left_order = request
            .index_snapshot
            .metadata
            .get(&left.source_id)
            .map(|entry| entry.rule.selection.order)
            .unwrap_or_default();
        let right_order = request
            .index_snapshot
            .metadata
            .get(&right.source_id)
            .map(|entry| entry.rule.selection.order)
            .unwrap_or_default();
        class_rank(left.class, left.mandatory, left.sticky)
            .cmp(&class_rank(right.class, right.mandatory, right.sticky))
            .then_with(|| right_order.cmp(&left_order))
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.provider_rank.cmp(&right.provider_rank))
            .then_with(|| left.source_id.cmp(&right.source_id))
            .then_with(|| continuation.activated.len().cmp(&continuation.activated.len()))
    }
}

fn class_rank(kind: ActivationSeedKind, mandatory: bool, sticky: bool) -> u8 {
    if sticky {
        0
    } else if mandatory {
        1
    } else {
        match kind {
            ActivationSeedKind::Constant => 3,
            ActivationSeedKind::TextMatch => 4,
            ActivationSeedKind::Provider => 5,
            _ => 2,
        }
    }
}

fn timed_state_is_cooling(
    source_id: &KnowledgeSourceId,
    states: &[ActivationTimedState],
    turn: crate::domain::ids::TurnNumber,
) -> bool {
    states
        .iter()
        .any(|state| state.source_id == *source_id && state.cooldown_through_turn.is_some_and(|value| value >= turn))
}

fn probability_admits(
    story_id: &str,
    turn: u64,
    source_id: &KnowledgeSourceId,
    probability: &u8,
    version: &[u8; 32],
) -> bool {
    if *probability == 0 {
        return false;
    }
    if *probability == 100 {
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
    sample < u16::from(*probability)
}

fn timed_state_delta(
    request: &ActivationRequest<'_>,
    rejections: &BTreeMap<ActivationRejectionReason, u32>,
) -> PendingActivationStateDelta {
    let _ = request;
    let _ = rejections;
    PendingActivationStateDelta::default()
}

fn reject(
    continuation: &mut ActivationContinuation,
    rejections: &mut BTreeMap<ActivationRejectionReason, u32>,
    source_id: KnowledgeSourceId,
    reason: ActivationRejectionReason,
) {
    continuation.terminal_rejections.insert(source_id, reason);
    increment(rejections, reason);
}

fn increment(rejections: &mut BTreeMap<ActivationRejectionReason, u32>, reason: ActivationRejectionReason) {
    *rejections.entry(reason).or_default() += 1;
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
}
