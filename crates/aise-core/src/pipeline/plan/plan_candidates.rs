use crate::core::{ActivatedKnowledge, FactId, RetrievalHint, RumorId, StoryPack};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CandidateStats {
    pub total: usize,
    pub offered: usize,
}

impl CandidateStats {
    pub fn dropped(self) -> usize {
        self.total.saturating_sub(self.offered)
    }

    pub fn truncated(self) -> bool {
        self.dropped() > 0
    }
}

pub(super) struct RetrievalCandidate<'a, I> {
    pub id: &'a I,
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) struct PlanCandidates<'a> {
    pub facts: Vec<RetrievalCandidate<'a, FactId>>,
    pub rumors: Vec<RetrievalCandidate<'a, RumorId>>,
    pub fact_stats: CandidateStats,
    pub rumor_stats: CandidateStats,
}

impl PlanCandidates<'_> {
    pub fn contains_fact(&self, id: &FactId) -> bool {
        self.facts.iter().any(|candidate| candidate.id == id)
    }

    pub fn contains_rumor(&self, id: &RumorId) -> bool {
        self.rumors.iter().any(|candidate| candidate.id == id)
    }
}

pub(super) fn collect_candidates<'a>(
    story_pack: &'a StoryPack,
    activated: &ActivatedKnowledge,
    limit: usize,
) -> PlanCandidates<'a> {
    let activated_fact_ids = activated.facts.iter().map(|fact| &fact.id).collect::<BTreeSet<_>>();
    let activated_rumor_ids = activated.rumors.iter().map(|rumor| &rumor.id).collect::<BTreeSet<_>>();
    let mut facts = story_pack
        .seed_facts
        .iter()
        .filter(|(id, _)| !activated_fact_ids.contains(*id))
        .map(|(id, body)| RetrievalCandidate {
            id,
            retrieval_hint: &body.retrieval,
        })
        .collect::<Vec<_>>();
    let mut rumors = story_pack
        .seed_rumors
        .iter()
        .filter(|(id, _)| !activated_rumor_ids.contains(*id))
        .map(|(id, body)| RetrievalCandidate {
            id,
            retrieval_hint: &body.retrieval,
        })
        .collect::<Vec<_>>();
    let fact_total = facts.len();
    let rumor_total = rumors.len();
    facts.truncate(limit);
    rumors.truncate(limit);

    PlanCandidates {
        fact_stats: CandidateStats {
            total: fact_total,
            offered: facts.len(),
        },
        rumor_stats: CandidateStats {
            total: rumor_total,
            offered: rumors.len(),
        },
        facts,
        rumors,
    }
}

#[cfg(test)]
#[path = "tests/plan_candidates_tests.rs"]
mod tests;
