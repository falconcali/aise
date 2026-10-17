use super::*;
use crate::core::tests::snake_pack;
use crate::core::{EntryOrigin, FactBody, FactEntry, RumorBody, RumorEntry, StoryContext};
use std::collections::BTreeMap;

fn context_with_ids(fact_ids: &[&str], rumor_ids: &[&str]) -> StoryContext {
    let pack = snake_pack();
    let fact_body = pack.seed_facts.values().next().expect("fixture fact").clone();
    let rumor_body = pack.seed_rumors.values().next().expect("fixture rumor").clone();
    let mut story_ctx = StoryContext::new();
    story_ctx.facts = fact_ids
        .iter()
        .map(|id| {
            let fact_id = FactId::try_new(*id).expect("valid fact id");
            (
                fact_id.clone(),
                FactEntry {
                    id: fact_id,
                    origin: EntryOrigin::Seed,
                    body: fact_body.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    story_ctx.rumors = rumor_ids
        .iter()
        .map(|id| {
            let rumor_id = RumorId::try_new(*id).expect("valid rumor id");
            (
                rumor_id.clone(),
                RumorEntry {
                    id: rumor_id,
                    origin: EntryOrigin::Seed,
                    body: rumor_body.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    story_ctx
}

fn activated_fact(id: &FactId, body: &FactBody) -> FactEntry {
    FactEntry {
        id: id.clone(),
        origin: EntryOrigin::Seed,
        body: body.clone(),
    }
}

fn activated_rumor(id: &RumorId, body: &RumorBody) -> RumorEntry {
    RumorEntry {
        id: id.clone(),
        origin: EntryOrigin::Seed,
        body: body.clone(),
    }
}

#[test]
fn all_entries_are_candidates_without_activation() {
    let story_ctx = context_with_ids(&["fact-b", "fact-a"], &["rumor-b", "rumor-a"]);
    let candidates = collect_candidates(&story_ctx, &ActivatedKnowledge::default(), usize::MAX);

    assert_eq!(
        candidates
            .facts
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["fact-a", "fact-b"]
    );
    assert_eq!(
        candidates
            .rumors
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["rumor-a", "rumor-b"]
    );
}

#[test]
fn candidates_exclude_activated_facts_and_rumors() {
    let story_ctx = context_with_ids(&["shared", "visible"], &["shared", "visible"]);
    let fact_id = FactId::try_new("shared").expect("valid fact id");
    let rumor_id = RumorId::try_new("shared").expect("valid rumor id");
    let activated = ActivatedKnowledge {
        facts: vec![activated_fact(
            &fact_id,
            &story_ctx.facts.get(&fact_id).expect("fixture fact").body,
        )],
        rumors: vec![activated_rumor(
            &rumor_id,
            &story_ctx.rumors.get(&rumor_id).expect("fixture rumor").body,
        )],
    };
    let candidates = collect_candidates(&story_ctx, &activated, usize::MAX);

    assert!(!candidates.contains_fact(&fact_id));
    assert!(!candidates.contains_rumor(&rumor_id));
    assert!(candidates.contains_fact(&FactId::try_new("visible").expect("valid fact id")));
    assert!(candidates.contains_rumor(&RumorId::try_new("visible").expect("valid rumor id")));
}

#[test]
fn activated_rumor_is_excluded_only_from_rumor_candidates() {
    let story_ctx = context_with_ids(&["shared"], &["shared", "visible"]);
    let rumor_id = RumorId::try_new("shared").expect("valid rumor id");
    let activated = ActivatedKnowledge {
        facts: Vec::new(),
        rumors: vec![activated_rumor(
            &rumor_id,
            &story_ctx.rumors.get(&rumor_id).expect("fixture rumor").body,
        )],
    };
    let candidates = collect_candidates(&story_ctx, &activated, usize::MAX);

    assert!(candidates.contains_fact(&FactId::try_new("shared").expect("valid fact id")));
    assert!(!candidates.contains_rumor(&rumor_id));
}

#[test]
fn empty_context_produces_empty_candidates() {
    let story_ctx = context_with_ids(&[], &[]);
    let candidates = collect_candidates(&story_ctx, &ActivatedKnowledge::default(), 4);

    assert!(candidates.facts.is_empty());
    assert!(candidates.rumors.is_empty());
}

#[test]
fn candidates_preserve_stable_id_order() {
    let story_ctx = context_with_ids(&["z", "a", "m"], &["y", "b", "n"]);
    let candidates = collect_candidates(&story_ctx, &ActivatedKnowledge::default(), usize::MAX);

    assert_eq!(
        candidates
            .facts
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "m", "z"]
    );
    assert_eq!(
        candidates
            .rumors
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "n", "y"]
    );
}

#[test]
fn candidates_truncate_each_kind_independently() {
    let story_ctx = context_with_ids(&["a", "b", "c"], &["a", "b"]);
    let candidates = collect_candidates(&story_ctx, &ActivatedKnowledge::default(), 2);

    assert_eq!(candidates.facts.len(), 2);
    assert_eq!(candidates.rumors.len(), 2);
}

#[test]
fn zero_limit_truncates_all_eligible_candidates() {
    let story_ctx = context_with_ids(&["fact"], &["rumor"]);
    let candidates = collect_candidates(&story_ctx, &ActivatedKnowledge::default(), 0);

    assert!(candidates.facts.is_empty());
    assert!(candidates.rumors.is_empty());
}
