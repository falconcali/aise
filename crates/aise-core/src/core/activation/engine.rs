use crate::core::activation::config::ActivationConfig;
use crate::core::activation::rule::{ActivationGroupKey, ActivationKey, ActivationProbability, ActivationRule};
use crate::core::activation::scan::{ScanBuffer, ScanFragment, ScanFragmentKind, normalize_text};
use crate::core::{BoundedText, StoryId, TurnNumber};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;

pub struct ActivationCandidate<'a, K> {
    pub key: K,
    pub rule: &'a ActivationRule,
    pub content: &'a BoundedText,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivationTurn<'a> {
    pub story_id: &'a StoryId,
    pub turn_number: &'a TurnNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationClass {
    Constant,
    TextMatch,
    Recursion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPatternKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationRejection {
    RecursionExcluded,
    RecursionLocked,
    SecondaryCondition,
    GroupLoser,
    Probability,
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationStopReason {
    Complete,
    RecursionExhausted,
    BudgetTrimmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationEvidence {
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub depth: u16,
    pub match_count: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivatedEntry<K> {
    pub key: K,
    pub class: ActivationClass,
    pub recursion_level: u16,
    pub evidence: Vec<ActivationEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationOutcome<K> {
    pub activated: Vec<ActivatedEntry<K>>,
    pub rejections: BTreeMap<ActivationRejection, u32>,
    pub recursion_level: u16,
    pub stop_reason: ActivationStopReason,
}

pub fn activate<K>(
    candidates: Vec<ActivationCandidate<'_, K>>,
    scan: &ScanBuffer,
    turn: ActivationTurn<'_>,
    config: &ActivationConfig,
) -> ActivationOutcome<K>
where
    K: Clone + Ord + Display,
{
    let mut session = Session::new(candidates, turn, config);
    session.ingest(scan.fragments(), true);
    session.run();
    session.finish()
}

struct Entry<'a, K> {
    candidate: ActivationCandidate<'a, K>,
    primary: Vec<String>,
    secondary: Vec<String>,
    scan_depth: u16,
    evidence: Vec<ActivationEvidence>,
}

impl<'a, K> Entry<'a, K> {
    fn new(candidate: ActivationCandidate<'a, K>, config: &ActivationConfig) -> Self {
        let match_rule = &candidate.rule.match_rule;
        let needles = |keys: &[ActivationKey]| {
            keys.iter()
                .map(|key| normalize_text(key.as_str(), match_rule.case_sensitive))
                .collect::<Vec<_>>()
        };
        let primary = needles(&match_rule.keys);
        let secondary = needles(&match_rule.secondary_keys);
        let scan_depth = match_rule.scan_depth.unwrap_or(config.scan_depth).min(config.max_scan_depth);
        Self {
            candidate,
            primary,
            secondary,
            scan_depth,
            evidence: Vec::new(),
        }
    }

    fn ingest(&mut self, fragment: &ScanFragment, base: bool) {
        if base && fragment.depth() > self.scan_depth {
            return;
        }
        let match_rule = &self.candidate.rule.match_rule;
        let options = (match_rule.case_sensitive, match_rule.match_whole_words);
        record_matches(
            &mut self.evidence,
            &self.primary,
            ActivationPatternKind::Primary,
            fragment,
            options,
        );
        record_matches(
            &mut self.evidence,
            &self.secondary,
            ActivationPatternKind::Secondary,
            fragment,
            options,
        );
    }

    fn matched(&self, kind: ActivationPatternKind) -> usize {
        self.evidence.iter().filter(|item| item.pattern_kind == kind).count()
    }

    fn recursion_only(&self) -> bool {
        self.evidence
            .iter()
            .filter(|item| item.pattern_kind == ActivationPatternKind::Primary)
            .all(|item| item.fragment_kind == ScanFragmentKind::RecursionContent)
    }

    fn best_evidence(&self) -> Option<(ScanFragmentKind, u16)> {
        self.evidence.iter().map(|item| (item.fragment_kind, item.depth)).min()
    }
}

type CandidateOrdering = (ActivationClass, Reverse<i32>, Reverse<u16>, Option<(ScanFragmentKind, u16)>);

#[derive(Debug, Clone, Copy)]
struct Candidate {
    index: usize,
    class: ActivationClass,
    score: u16,
}

struct Session<'a, 'c, K> {
    entries: Vec<Entry<'a, K>>,
    turn: ActivationTurn<'c>,
    config: &'c ActivationConfig,
    level: u16,
    activated: Vec<ActivatedEntry<K>>,
    activated_indices: BTreeSet<usize>,
    terminal: BTreeMap<usize, ActivationRejection>,
    deferred: BTreeMap<usize, ActivationRejection>,
    group_winners: BTreeMap<ActivationGroupKey, usize>,
    knowledge_bytes: usize,
    budget_trimmed: bool,
    recursion_exhausted: bool,
}

impl<'a, 'c, K> Session<'a, 'c, K>
where
    K: Clone + Ord + Display,
{
    fn new(
        candidates: Vec<ActivationCandidate<'a, K>>,
        turn: ActivationTurn<'c>,
        config: &'c ActivationConfig,
    ) -> Self {
        Self {
            entries: candidates.into_iter().map(|candidate| Entry::new(candidate, config)).collect(),
            turn,
            config,
            level: 0,
            activated: Vec::new(),
            activated_indices: BTreeSet::new(),
            terminal: BTreeMap::new(),
            deferred: BTreeMap::new(),
            group_winners: BTreeMap::new(),
            knowledge_bytes: 0,
            budget_trimmed: false,
            recursion_exhausted: false,
        }
    }

    fn run(&mut self) {
        loop {
            let candidates = self.collect();
            if candidates.is_empty() {
                return;
            }
            let candidates = self.resolve_groups(candidates);
            let fragments = self.admit(candidates);
            if fragments.is_empty() {
                return;
            }
            if self.level >= self.config.max_recursion_steps {
                self.recursion_exhausted = true;
                return;
            }
            self.level = self.level.saturating_add(1);
            self.ingest(&fragments, false);
        }
    }

    fn ingest(&mut self, fragments: &[ScanFragment], base: bool) {
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if self.activated_indices.contains(&index) || self.terminal.contains_key(&index) {
                continue;
            }
            for fragment in fragments {
                entry.ingest(fragment, base);
            }
        }
    }

    fn collect(&mut self) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if self.activated_indices.contains(&index) || self.terminal.contains_key(&index) {
                continue;
            }
            let rule = entry.candidate.rule;
            if !rule.mode.enabled {
                continue;
            }
            if rule.mode.constant {
                if self.level == 0 {
                    candidates.push(Candidate {
                        index,
                        class: ActivationClass::Constant,
                        score: 0,
                    });
                }
                continue;
            }
            let primary = entry.matched(ActivationPatternKind::Primary);
            if primary == 0 {
                continue;
            }
            if rule
                .recursion
                .delay_until_recursion
                .is_some_and(|level| self.level < level.get())
            {
                self.deferred.insert(index, ActivationRejection::RecursionLocked);
                continue;
            }
            let recursion_only = entry.recursion_only();
            if recursion_only && rule.recursion.exclude_recursion {
                self.terminal.insert(index, ActivationRejection::RecursionExcluded);
                continue;
            }
            let secondary = entry.matched(ActivationPatternKind::Secondary);
            let logic = rule.match_rule.secondary_logic;
            if !logic.holds(secondary, entry.secondary.len()) {
                self.deferred.insert(index, ActivationRejection::SecondaryCondition);
                continue;
            }
            self.deferred.remove(&index);
            let score = primary.saturating_add(if logic.scores_matches() { secondary } else { 0 });
            candidates.push(Candidate {
                index,
                class: if recursion_only {
                    ActivationClass::Recursion
                } else {
                    ActivationClass::TextMatch
                },
                score: u16::try_from(score).unwrap_or(u16::MAX),
            });
        }
        candidates
    }

    fn resolve_groups(&mut self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        let groups = candidates
            .iter()
            .flat_map(|candidate| self.rule(candidate.index).selection.groups.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut eligible = candidates;
        for group in groups {
            let members = eligible
                .iter()
                .filter(|candidate| self.rule(candidate.index).selection.groups.contains(&group))
                .copied()
                .collect::<Vec<_>>();
            if members.is_empty() {
                continue;
            }
            let winner = match self.group_winners.get(&group) {
                Some(_) => None,
                None => self.group_winner(&group, &members),
            };
            for member in &members {
                if Some(member.index) != winner {
                    self.terminal.insert(member.index, ActivationRejection::GroupLoser);
                }
            }
            eligible.retain(|candidate| {
                Some(candidate.index) == winner || !members.iter().any(|member| member.index == candidate.index)
            });
            if let Some(index) = winner {
                self.group_winners.insert(group, index);
            }
        }
        eligible
    }

    fn group_winner(&self, group: &ActivationGroupKey, members: &[Candidate]) -> Option<usize> {
        let rank = |candidate: &Candidate| {
            let selection = &self.rule(candidate.index).selection;
            let score = if selection.use_group_scoring {
                candidate.score
            } else {
                0
            };
            (selection.group_override, score, selection.order)
        };
        let best = members.iter().map(rank).max()?;
        let mut tied = members
            .iter()
            .filter(|candidate| rank(candidate) == best)
            .map(|candidate| candidate.index)
            .collect::<Vec<_>>();
        tied.sort_by(|left, right| self.key(*left).cmp(self.key(*right)));
        let weight = |index: usize| u64::from(self.rule(index).selection.group_weight);
        let total = tied.iter().map(|index| weight(*index)).sum::<u64>();
        if total == 0 {
            return tied.first().copied();
        }
        let mut hasher = Sha256::new();
        hash_field(&mut hasher, b"aise.activation.group.v1");
        self.hash_turn(&mut hasher);
        hash_field(&mut hasher, group.as_str().as_bytes());
        for index in &tied {
            hash_field(&mut hasher, self.key(*index).to_string().as_bytes());
            hash_field(&mut hasher, &weight(*index).to_be_bytes());
        }
        let mut sample = digest_u64(hasher) % total;
        for index in tied {
            if sample < weight(index) {
                return Some(index);
            }
            sample -= weight(index);
        }
        None
    }

    fn admit(&mut self, mut candidates: Vec<Candidate>) -> Vec<ScanFragment> {
        candidates.sort_by(|left, right| {
            self.ordering(left)
                .cmp(&self.ordering(right))
                .then_with(|| self.key(left.index).cmp(self.key(right.index)))
        });
        let mut fragments = Vec::new();
        for candidate in candidates {
            let entry = &self.entries[candidate.index];
            let rule = entry.candidate.rule;
            if !self.probability_admits(candidate.index, rule.selection.probability) {
                self.terminal.insert(candidate.index, ActivationRejection::Probability);
                continue;
            }
            let content = entry.candidate.content;
            if self.activated.len() >= self.config.max_activated_entries
                || self.knowledge_bytes.saturating_add(content.len()) > self.config.max_knowledge_bytes
            {
                self.terminal.insert(candidate.index, ActivationRejection::Budget);
                self.budget_trimmed = true;
                continue;
            }
            self.knowledge_bytes = self.knowledge_bytes.saturating_add(content.len());
            self.activated_indices.insert(candidate.index);
            self.activated.push(ActivatedEntry {
                key: entry.candidate.key.clone(),
                class: candidate.class,
                recursion_level: self.level,
                evidence: entry.evidence.clone(),
            });
            if !rule.recursion.prevent_recursion {
                fragments.push(ScanFragment::new(
                    ScanFragmentKind::RecursionContent,
                    self.level.saturating_add(1),
                    content.as_str(),
                ));
            }
        }
        fragments
    }

    fn finish(self) -> ActivationOutcome<K> {
        let deferred = self
            .deferred
            .iter()
            .filter(|(index, _)| !self.activated_indices.contains(*index) && !self.terminal.contains_key(*index));
        let mut rejections = BTreeMap::new();
        for reason in self.terminal.values().chain(deferred.map(|(_, reason)| reason)) {
            *rejections.entry(*reason).or_insert(0u32) += 1;
        }
        let stop_reason = if self.recursion_exhausted {
            ActivationStopReason::RecursionExhausted
        } else if self.budget_trimmed {
            ActivationStopReason::BudgetTrimmed
        } else {
            ActivationStopReason::Complete
        };
        ActivationOutcome {
            activated: self.activated,
            rejections,
            recursion_level: self.level,
            stop_reason,
        }
    }

    fn ordering(&self, candidate: &Candidate) -> CandidateOrdering {
        let entry = &self.entries[candidate.index];
        (
            candidate.class,
            Reverse(entry.candidate.rule.selection.order),
            Reverse(candidate.score),
            entry.best_evidence(),
        )
    }

    fn probability_admits(&self, index: usize, probability: ActivationProbability) -> bool {
        match probability.percent() {
            0 => false,
            100.. => true,
            percent => {
                let mut hasher = Sha256::new();
                hash_field(&mut hasher, b"aise.activation.probability.v1");
                self.hash_turn(&mut hasher);
                hash_field(&mut hasher, self.key(index).to_string().as_bytes());
                digest_u64(hasher) % 100 < u64::from(percent)
            }
        }
    }

    fn hash_turn(&self, hasher: &mut Sha256) {
        hash_field(hasher, self.turn.story_id.as_str().as_bytes());
        hash_field(hasher, &self.turn.turn_number.value().to_be_bytes());
    }

    fn rule(&self, index: usize) -> &ActivationRule {
        self.entries[index].candidate.rule
    }

    fn key(&self, index: usize) -> &K {
        &self.entries[index].candidate.key
    }
}

fn record_matches(
    evidence: &mut Vec<ActivationEvidence>,
    needles: &[String],
    pattern_kind: ActivationPatternKind,
    fragment: &ScanFragment,
    (case_sensitive, whole_words): (bool, bool),
) {
    for (ordinal, needle) in needles.iter().enumerate() {
        let pattern_ordinal = u16::try_from(ordinal).unwrap_or(u16::MAX);
        if evidence
            .iter()
            .any(|item| item.pattern_kind == pattern_kind && item.pattern_ordinal == pattern_ordinal)
        {
            continue;
        }
        let count = fragment.count(needle, case_sensitive, whole_words);
        if count == 0 {
            continue;
        }
        evidence.push(ActivationEvidence {
            pattern_kind,
            pattern_ordinal,
            fragment_kind: fragment.kind(),
            depth: fragment.depth(),
            match_count: u16::try_from(count).unwrap_or(u16::MAX),
        });
    }
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn digest_u64(hasher: Sha256) -> u64 {
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

#[cfg(test)]
#[path = "tests/engine_tests.rs"]
mod tests;
