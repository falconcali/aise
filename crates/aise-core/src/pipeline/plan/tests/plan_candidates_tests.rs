use super::*;
use crate::core::tests::snake_pack;
use crate::core::{EntryOrigin, FactBody, FactEntry, RumorBody, RumorEntry};
use std::collections::BTreeMap;

fn pack_with_ids(fact_ids: &[&str], rumor_ids: &[&str]) -> StoryPack {
    let mut pack = snake_pack();
    let fact_body = pack.seed_facts.values().next().expect("fixture fact").clone();
    let rumor_body = pack.seed_rumors.values().next().expect("fixture rumor").clone();
    pack.seed_facts = fact_ids
        .iter()
        .map(|id| (FactId::try_new(*id).expect("valid fact id"), fact_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack.seed_rumors = rumor_ids
        .iter()
        .map(|id| (RumorId::try_new(*id).expect("valid rumor id"), rumor_body.clone()))
        .collect::<BTreeMap<_, _>>();
    pack
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
    let pack = pack_with_ids(&["fact-b", "fact-a"], &["rumor-b", "rumor-a"]);
    let candidates = collect_candidates(&pack, &ActivatedKnowledge::default(), usize::MAX);

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
    let pack = pack_with_ids(&["shared", "visible"], &["shared", "visible"]);
    let fact_id = FactId::try_new("shared").expect("valid fact id");
    let rumor_id = RumorId::try_new("shared").expect("valid rumor id");
    let activated = ActivatedKnowledge {
        facts: vec![activated_fact(
            &fact_id,
            pack.seed_facts.get(&fact_id).expect("fixture fact"),
        )],
        rumors: vec![activated_rumor(
            &rumor_id,
            pack.seed_rumors.get(&rumor_id).expect("fixture rumor"),
        )],
    };
    let candidates = collect_candidates(&pack, &activated, usize::MAX);

    assert!(!candidates.contains_fact(&fact_id));
    assert!(!candidates.contains_rumor(&rumor_id));
    assert!(candidates.contains_fact(&FactId::try_new("visible").expect("valid fact id")));
    assert!(candidates.contains_rumor(&RumorId::try_new("visible").expect("valid rumor id")));
}

#[test]
fn activated_rumor_is_excluded_only_from_rumor_candidates() {
    let pack = pack_with_ids(&["shared"], &["shared", "visible"]);
    let rumor_id = RumorId::try_new("shared").expect("valid rumor id");
    let activated = ActivatedKnowledge {
        facts: Vec::new(),
        rumors: vec![activated_rumor(
            &rumor_id,
            pack.seed_rumors.get(&rumor_id).expect("fixture rumor"),
        )],
    };
    let candidates = collect_candidates(&pack, &activated, usize::MAX);

    assert!(candidates.contains_fact(&FactId::try_new("shared").expect("valid fact id")));
    assert!(!candidates.contains_rumor(&rumor_id));
}

#[test]
fn empty_pack_produces_empty_candidates() {
    let pack = pack_with_ids(&[], &[]);
    let candidates = collect_candidates(&pack, &ActivatedKnowledge::default(), 4);

    assert!(candidates.facts.is_empty());
    assert!(candidates.rumors.is_empty());
}

#[test]
fn candidates_preserve_stable_id_order() {
    let pack = pack_with_ids(&["z", "a", "m"], &["y", "b", "n"]);
    let candidates = collect_candidates(&pack, &ActivatedKnowledge::default(), usize::MAX);

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
    let pack = pack_with_ids(&["a", "b", "c"], &["a", "b"]);
    let candidates = collect_candidates(&pack, &ActivatedKnowledge::default(), 2);

    assert_eq!(candidates.facts.len(), 2);
    assert_eq!(candidates.rumors.len(), 2);
}

#[test]
fn zero_limit_truncates_all_eligible_candidates() {
    let pack = pack_with_ids(&["fact"], &["rumor"]);
    let candidates = collect_candidates(&pack, &ActivatedKnowledge::default(), 0);

    assert!(candidates.facts.is_empty());
    assert!(candidates.rumors.is_empty());
}
