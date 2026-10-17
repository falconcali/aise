use crate::core::{ActivatedKnowledge, FactId, RetrievalHint, RumorId, StoryContext};
use std::collections::BTreeSet;

pub(super) struct RetrievalCandidate<'a, I> {
    pub id: &'a I,
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) struct PlanCandidates<'a> {
    pub facts: Vec<RetrievalCandidate<'a, FactId>>,
    pub rumors: Vec<RetrievalCandidate<'a, RumorId>>,
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
    story_ctx: &'a StoryContext,
    activated: &ActivatedKnowledge,
    limit: usize,
) -> PlanCandidates<'a> {
    let activated_fact_ids = activated.facts.iter().map(|fact| &fact.id).collect::<BTreeSet<_>>();
    let activated_rumor_ids = activated.rumors.iter().map(|rumor| &rumor.id).collect::<BTreeSet<_>>();
    let facts = story_ctx
        .facts
        .iter()
        .filter(|(id, _)| !activated_fact_ids.contains(*id))
        .take(limit)
        .map(|(id, entry)| RetrievalCandidate {
            id,
            retrieval_hint: &entry.body.retrieval,
        })
        .collect::<Vec<_>>();
    let rumors = story_ctx
        .rumors
        .iter()
        .filter(|(id, _)| !activated_rumor_ids.contains(*id))
        .take(limit)
        .map(|(id, entry)| RetrievalCandidate {
            id,
            retrieval_hint: &entry.body.retrieval,
        })
        .collect::<Vec<_>>();

    PlanCandidates { facts, rumors }
}

#[cfg(test)]
#[path = "tests/plan_candidates_tests.rs"]
mod tests;
