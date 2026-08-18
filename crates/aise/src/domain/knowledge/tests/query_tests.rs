use super::*;
use std::collections::BTreeSet;

#[test]
fn seed_knowledge_ids_are_short_and_stable() {
    let kinds = [
        KnowledgeKind::Fact,
        KnowledgeKind::Fact,
        KnowledgeKind::Rumor,
        KnowledgeKind::Memory,
    ];

    let allocation = allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &kinds).unwrap();

    let ids: Vec<String> = allocation.assigned.iter().map(|id| id.as_str().to_owned()).collect();
    assert_eq!(ids, vec!["fact_0001", "fact_0002", "rumor_0003", "memory_0004"]);
    assert_eq!(allocation.new_high_water.get(), 4);
    for id in &ids {
        assert!(id.len() <= 11, "seed knowledge id must stay short: {id}");
    }

    let repeated = allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &kinds).unwrap();
    assert_eq!(
        allocation.assigned, repeated.assigned,
        "seed allocation must be a stable function of its inputs"
    );
}

#[test]
fn runtime_knowledge_ids_use_story_local_sequence() {
    let seed_allocation =
        allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &[KnowledgeKind::Fact, KnowledgeKind::Rumor]).unwrap();
    assert_eq!(seed_allocation.new_high_water.get(), 2);

    let runtime_allocation =
        allocate_knowledge_ids(seed_allocation.new_high_water, &[KnowledgeKind::Fact, KnowledgeKind::Memory]).unwrap();

    let ids: Vec<String> = runtime_allocation.assigned.iter().map(|id| id.as_str().to_owned()).collect();
    assert_eq!(ids, vec!["fact_0003", "memory_0004"]);
    assert_eq!(runtime_allocation.new_high_water.get(), 4);
}

#[test]
fn knowledge_id_allocation_is_retry_stable() {
    let base = KnowledgeIdHighWater::new(9);
    let kinds = [KnowledgeKind::Rumor, KnowledgeKind::Fact, KnowledgeKind::Memory];

    let first = allocate_knowledge_ids(base, &kinds).unwrap();
    let retried = allocate_knowledge_ids(base, &kinds).unwrap();

    assert_eq!(first.assigned, retried.assigned);
    assert_eq!(first.new_high_water, retried.new_high_water);
}

#[test]
fn knowledge_id_sequence_never_reuses_deleted_value() {
    let after_seed = allocate_knowledge_ids(KnowledgeIdHighWater::zero(), &[KnowledgeKind::Fact; 5]).unwrap();
    assert_eq!(after_seed.new_high_water.get(), 5);

    let next = allocate_knowledge_ids(after_seed.new_high_water, &[KnowledgeKind::Fact]).unwrap();

    assert_eq!(next.assigned[0].as_str(), "fact_0006");
    let previously_used: BTreeSet<_> = after_seed.assigned.iter().map(|id| id.as_str().to_owned()).collect();
    assert!(!previously_used.contains(next.assigned[0].as_str()));
}

#[test]
fn allocate_knowledge_ids_switches_to_unpadded_form_after_nine_thousand_nine_hundred_ninety_nine() {
    let allocation =
        allocate_knowledge_ids(KnowledgeIdHighWater::new(9998), &[KnowledgeKind::Fact, KnowledgeKind::Fact]).unwrap();
    let ids: Vec<String> = allocation.assigned.iter().map(|id| id.as_str().to_owned()).collect();
    assert_eq!(ids, vec!["fact_9999", "fact_10000"]);
}
