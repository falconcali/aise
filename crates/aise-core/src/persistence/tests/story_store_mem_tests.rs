use super::*;
use crate::core::tests::{snake_pack, story_context, turn_evaluation};
use crate::core::{
    ActivationRule, BoundedText, Change, EntryOrigin, FactBody, FactChangeOp, FactEntry, FactId, IdempotencyKey,
    KnowledgeChange, MemoryBody, MemoryEntry, MemoryId, NarrativeNodeId, NarrativeState, NodeStatus, PlayerId,
    RetrievalHint, RoleEntry, RoleId, RoleMemoryChangeOp, RumorBody, RumorChangeOp, RumorEntry, RumorId, StoryCommit,
    StoryCreateBundle, StoryId, StoryInstanceSpec, StoryPackSnapshot, StorySummary, Turn, TurnNumber, TurnSegment,
    TurnStatus,
};
use crate::persistence::StoreType;
use std::collections::BTreeMap;
use std::sync::Arc;

fn story_id(value: &str) -> StoryId {
    StoryId::try_new(value).expect("test story id should be valid")
}

fn idempotency_key(value: &str) -> IdempotencyKey {
    IdempotencyKey::try_new(value).expect("test idempotency key should be valid")
}

fn fact_id(value: &str) -> FactId {
    FactId::try_new(value).expect("test fact id should be valid")
}

fn rumor_id(value: &str) -> RumorId {
    RumorId::try_new(value).expect("test rumor id should be valid")
}

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn memory_id(value: &str) -> MemoryId {
    MemoryId::try_new(value).expect("test memory id should be valid")
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should be valid")
}

fn fact_body(content: &str) -> FactBody {
    FactBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new(format!("hint-{content}")).expect("valid hint"),
    }
}

fn rumor_body(content: &str) -> RumorBody {
    RumorBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new(format!("hint-{content}")).expect("valid hint"),
    }
}

fn memory_body(content: &str) -> MemoryBody {
    MemoryBody {
        content: text(content),
        activation: ActivationRule::default(),
        retrieval: RetrievalHint::try_new(format!("hint-{content}")).expect("valid hint"),
    }
}

fn story_spec(value: &str) -> StoryInstanceSpec {
    story_spec_with_narrative(value, NarrativeState::default())
}

fn story_spec_with_narrative(value: &str, narrative: NarrativeState) -> StoryInstanceSpec {
    let pack = snake_pack();
    let snapshot = StoryPackSnapshot::from_pack(pack.clone()).expect("test pack snapshot should build");
    let bundle = StoryCreateBundle::assemble(
        snapshot,
        story_id(value),
        Arc::new(pack.opening),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        PlayerId::try_new("player").expect("test player id should be valid"),
        RoleId::try_new("protagonist").expect("test role id should be valid"),
        narrative,
    );
    bundle.into_parts().1
}

fn seeded_story_spec(value: &str) -> StoryInstanceSpec {
    let pack = snake_pack();
    let role_body = pack.roles.values().next().expect("fixture role").clone();
    let facts = BTreeMap::from([(
        fact_id("seed-fact"),
        FactEntry {
            id: fact_id("seed-fact"),
            origin: EntryOrigin::Seed,
            body: fact_body("seed fact"),
        },
    )]);
    let rumors = BTreeMap::from([(
        rumor_id("seed-rumor"),
        RumorEntry {
            id: rumor_id("seed-rumor"),
            origin: EntryOrigin::Seed,
            body: rumor_body("seed rumor"),
        },
    )]);
    let roles = BTreeMap::from([(
        role_id("npc"),
        RoleEntry {
            id: role_id("npc"),
            origin: EntryOrigin::Seed,
            spec: role_body.spec,
            profile: role_body.default_profile,
            memories: BTreeMap::from([(
                memory_id("seed-mem"),
                MemoryEntry {
                    id: memory_id("seed-mem"),
                    origin: EntryOrigin::Seed,
                    body: memory_body("seed memory"),
                },
            )]),
        },
    )]);
    let snapshot = StoryPackSnapshot::from_pack(pack.clone()).expect("test pack snapshot should build");
    StoryCreateBundle::assemble(
        snapshot,
        story_id(value),
        Arc::new(pack.opening),
        roles,
        facts,
        rumors,
        PlayerId::try_new("player").expect("test player id should be valid"),
        role_id("npc"),
        NarrativeState::default(),
    )
    .into_parts()
    .1
}

fn commit(story_id: &StoryId, turn_number: u64, key: &str, summary: Change<StorySummary>) -> StoryCommit {
    commit_with_knowledge(story_id, turn_number, key, summary, KnowledgeChange::default())
}

fn commit_with_knowledge(
    story_id: &StoryId,
    turn_number: u64,
    key: &str,
    summary: Change<StorySummary>,
    knowledge_change: KnowledgeChange,
) -> StoryCommit {
    StoryCommit {
        story_id: story_id.clone(),
        turn: Turn {
            turn_number: TurnNumber::new(turn_number),
            idempotency_key: idempotency_key(key),
            player_contribution: crate::core::PlayerContribution {
                raw: format!("raw-{turn_number}"),
                processed: format!("processed-{turn_number}"),
            },
            turn_segment: TurnSegment::new(format!("segment-{turn_number}")),
            knowledge_change,
            turn_evaluation: turn_evaluation(),
            turn_status: TurnStatus::Pass,
        },
        summary,
        narrative: Change::Unchanged,
        life_cycle: Change::Unchanged,
    }
}

#[tokio::test]
async fn creates_and_loads_story_context_and_info() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    let info = store.create(spec.clone()).await.expect("story creation should succeed");

    assert_eq!(info.story_id.as_str(), "story-1");
    assert!(matches!(info.life_cycle, crate::core::StoryLifeCycle::Active));

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    let fixture = story_context();
    assert_eq!(&loaded.story_id, spec.story_id());
    assert_eq!(loaded.player, role_id("protagonist"));
    assert_eq!(loaded.opening.as_str(), fixture.opening.as_str());
    assert_eq!(loaded.turn_number.value(), 0);
    assert!(loaded.summary.is_none());
    assert!(loaded.recent_turns.is_empty());

    let loaded_info = store.get_info(spec.story_id()).await.expect("story info should load");
    assert_eq!(loaded_info.story_id, info.story_id);
    assert_eq!(loaded_info.player_id.as_str(), "player");
    assert_eq!(loaded_info.created_at, info.created_at);
}

#[tokio::test]
async fn rejects_duplicate_creation_and_missing_story() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");

    let duplicate = store.create(spec).await.expect_err("duplicate should fail");
    assert!(matches!(duplicate, PersistenceError::ConstraintViolation { .. }));

    let missing = store.load(&story_id("missing")).await.expect_err("missing story should fail");
    assert!(matches!(missing, PersistenceError::NotFound));
}

#[tokio::test]
async fn commits_turn_and_returns_idempotent_result() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");
    let first = commit(spec.story_id(), 1, "key-1", Change::Unchanged);

    let committed = store.commit(first.clone()).await.expect("commit should succeed");
    assert_eq!(committed.turn.turn_number.value(), 1);

    let replayed = store.commit(first).await.expect("replayed commit should succeed");
    assert_eq!(replayed.turn.turn_number.value(), 1);

    let found = store
        .find_committed(spec.story_id(), &idempotency_key("key-1"))
        .await
        .expect("idempotency lookup should succeed")
        .expect("committed turn should be found");
    assert_eq!(found.turn.turn_number.value(), 1);

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert_eq!(loaded.turn_number.value(), 1);
    assert_eq!(loaded.recent_turns.len(), 1);
}

#[tokio::test]
async fn removes_turns_covered_by_summary_before_window_trim() {
    let store = StoryStoreMem::with_config(StoryStoreConfig {
        store_type: StoreType::Memory,
        max_recent_turns: 8,
    });
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");

    for turn_number in 1..=4 {
        let turn = commit(spec.story_id(), turn_number, &format!("key-{turn_number}"), Change::Unchanged);
        store.commit(turn).await.expect("turn commit should succeed");
    }

    let summary = StorySummary {
        text: "summary".to_owned(),
        covered_through: TurnNumber::new(3),
    };
    let turn = commit(spec.story_id(), 5, "key-5", Change::Replaced(summary));
    store.commit(turn).await.expect("summary commit should succeed");

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert_eq!(
        loaded.summary.as_ref().expect("summary should exist").covered_through.value(),
        3
    );
    assert_eq!(
        loaded
            .recent_turns
            .iter()
            .map(|turn| turn.turn_number.value())
            .collect::<Vec<_>>(),
        vec![4, 5]
    );
}

#[tokio::test]
async fn rejects_invalid_turn_number_and_future_summary() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");

    let invalid_turn = commit(spec.story_id(), 2, "key-2", Change::Unchanged);
    let error = store.commit(invalid_turn).await.expect_err("turn conflict should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));

    let future_summary = commit(
        spec.story_id(),
        1,
        "key-1",
        Change::Replaced(StorySummary {
            text: "future".to_owned(),
            covered_through: TurnNumber::new(2),
        }),
    );
    let error = store.commit(future_summary).await.expect_err("future summary should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn creation_keeps_initial_narrative_state() {
    let store = StoryStoreMem::new();
    let mut narrative = NarrativeState::default();
    narrative.node_status.insert(
        NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        NodeStatus::Active,
    );
    let spec = story_spec_with_narrative("story-1", narrative.clone());
    store.create(spec.clone()).await.expect("story creation should succeed");

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert_eq!(loaded.narrative, narrative);
}

#[tokio::test]
async fn commit_replaces_narrative_state() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");
    let mut narrative = NarrativeState::default();
    narrative.node_status.insert(
        NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        NodeStatus::Completed,
    );
    let mut turn = commit(spec.story_id(), 1, "key-1", Change::Unchanged);
    turn.narrative = Change::Replaced(narrative.clone());

    store.commit(turn).await.expect("commit should succeed");

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert_eq!(loaded.narrative, narrative);
}

#[tokio::test]
async fn completing_story_updates_life_cycle_and_rejects_further_turns() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");
    let mut final_turn = commit(spec.story_id(), 1, "key-1", Change::Unchanged);
    final_turn.life_cycle = Change::Replaced(crate::core::StoryLifeCycle::Completed);
    store.commit(final_turn.clone()).await.expect("final commit should succeed");

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert_eq!(loaded.life_cycle, crate::core::StoryLifeCycle::Completed);
    let info = store.get_info(spec.story_id()).await.expect("story info should load");
    assert_eq!(info.life_cycle, crate::core::StoryLifeCycle::Completed);

    let replayed = store
        .commit(final_turn)
        .await
        .expect("replaying the final commit stays idempotent");
    assert_eq!(replayed.turn.turn_number.value(), 1);

    let next_turn = commit(spec.story_id(), 2, "key-2", Change::Unchanged);
    let error = store
        .commit(next_turn)
        .await
        .expect_err("completed story should reject new turns");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));
}

#[tokio::test]
async fn commit_applies_knowledge_change_and_preserves_entry_origins() {
    let store = StoryStoreMem::new();
    let spec = seeded_story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");

    let turn_1_change = KnowledgeChange {
        facts: vec![
            FactChangeOp::Add {
                id: fact_id("turn-fact"),
                body: fact_body("turn 1 fact"),
            },
            FactChangeOp::Update {
                id: fact_id("seed-fact"),
                body: fact_body("updated seed fact"),
            },
        ],
        rumors: vec![
            RumorChangeOp::Add {
                id: rumor_id("turn-rumor"),
                body: rumor_body("turn 1 rumor"),
            },
            RumorChangeOp::Remove {
                id: rumor_id("seed-rumor"),
            },
        ],
        memories: vec![
            RoleMemoryChangeOp::Add {
                role_id: role_id("npc"),
                id: memory_id("turn-mem"),
                body: memory_body("turn 1 memory"),
            },
            RoleMemoryChangeOp::Update {
                role_id: role_id("npc"),
                id: memory_id("seed-mem"),
                body: memory_body("updated seed memory"),
            },
        ],
    };
    store
        .commit(commit_with_knowledge(
            spec.story_id(),
            1,
            "key-1",
            Change::Unchanged,
            turn_1_change,
        ))
        .await
        .expect("turn 1 commit should succeed");

    let turn_2_change = KnowledgeChange {
        facts: vec![FactChangeOp::Update {
            id: fact_id("turn-fact"),
            body: fact_body("updated turn 1 fact"),
        }],
        rumors: vec![RumorChangeOp::Update {
            id: rumor_id("turn-rumor"),
            body: rumor_body("updated turn 1 rumor"),
        }],
        memories: vec![RoleMemoryChangeOp::Update {
            role_id: role_id("npc"),
            id: memory_id("turn-mem"),
            body: memory_body("updated turn 1 memory"),
        }],
    };
    store
        .commit(commit_with_knowledge(
            spec.story_id(),
            2,
            "key-2",
            Change::Unchanged,
            turn_2_change,
        ))
        .await
        .expect("turn 2 commit should succeed");

    let loaded = store.load(spec.story_id()).await.expect("story should load");
    assert!(matches!(loaded.facts[&fact_id("seed-fact")].origin, EntryOrigin::Seed));
    assert_eq!(loaded.facts[&fact_id("seed-fact")].body.content.as_str(), "updated seed fact");
    assert!(matches!(
        &loaded.facts[&fact_id("turn-fact")].origin,
        EntryOrigin::Turn(t) if t.value() == 1
    ));
    assert_eq!(loaded.facts[&fact_id("turn-fact")].body.content.as_str(), "updated turn 1 fact");

    assert!(!loaded.rumors.contains_key(&rumor_id("seed-rumor")));
    assert!(matches!(
        &loaded.rumors[&rumor_id("turn-rumor")].origin,
        EntryOrigin::Turn(t) if t.value() == 1
    ));
    assert_eq!(
        loaded.rumors[&rumor_id("turn-rumor")].body.content.as_str(),
        "updated turn 1 rumor"
    );

    let npc_memories = &loaded.roles[&role_id("npc")].memories;
    assert!(matches!(npc_memories[&memory_id("seed-mem")].origin, EntryOrigin::Seed));
    assert_eq!(
        npc_memories[&memory_id("seed-mem")].body.content.as_str(),
        "updated seed memory"
    );
    assert!(matches!(
        &npc_memories[&memory_id("turn-mem")].origin,
        EntryOrigin::Turn(t) if t.value() == 1
    ));
    assert_eq!(
        npc_memories[&memory_id("turn-mem")].body.content.as_str(),
        "updated turn 1 memory"
    );
}

#[tokio::test]
async fn commit_rejects_invalid_knowledge_change_without_mutating_context() {
    let store = StoryStoreMem::new();
    let spec = seeded_story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");
    let before = serde_json::to_value(store.load(spec.story_id()).await.expect("story should load"))
        .expect("before context serializes");

    let invalid_changes = [
        KnowledgeChange {
            facts: vec![
                FactChangeOp::Add {
                    id: fact_id("dup"),
                    body: fact_body("one"),
                },
                FactChangeOp::Add {
                    id: fact_id("dup"),
                    body: fact_body("two"),
                },
            ],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            facts: vec![FactChangeOp::Add {
                id: fact_id("seed-fact"),
                body: fact_body("existing"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            facts: vec![FactChangeOp::Update {
                id: fact_id("missing-fact"),
                body: fact_body("missing"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            rumors: vec![
                RumorChangeOp::Update {
                    id: rumor_id("seed-rumor"),
                    body: rumor_body("one"),
                },
                RumorChangeOp::Remove {
                    id: rumor_id("seed-rumor"),
                },
            ],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            rumors: vec![RumorChangeOp::Add {
                id: rumor_id("seed-rumor"),
                body: rumor_body("existing"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            rumors: vec![RumorChangeOp::Update {
                id: rumor_id("missing-rumor"),
                body: rumor_body("missing"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            rumors: vec![RumorChangeOp::Remove {
                id: rumor_id("missing-rumor"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            memories: vec![
                RoleMemoryChangeOp::Add {
                    role_id: role_id("npc"),
                    id: memory_id("dup-mem"),
                    body: memory_body("one"),
                },
                RoleMemoryChangeOp::Add {
                    role_id: role_id("npc"),
                    id: memory_id("dup-mem"),
                    body: memory_body("two"),
                },
            ],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            memories: vec![RoleMemoryChangeOp::Add {
                role_id: role_id("unknown-role"),
                id: memory_id("new-mem"),
                body: memory_body("unknown role"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            memories: vec![RoleMemoryChangeOp::Add {
                role_id: role_id("npc"),
                id: memory_id("seed-mem"),
                body: memory_body("existing"),
            }],
            ..KnowledgeChange::default()
        },
        KnowledgeChange {
            memories: vec![RoleMemoryChangeOp::Update {
                role_id: role_id("npc"),
                id: memory_id("missing-mem"),
                body: memory_body("missing"),
            }],
            ..KnowledgeChange::default()
        },
    ];

    for (index, invalid) in invalid_changes.into_iter().enumerate() {
        let error = store
            .commit(commit_with_knowledge(
                spec.story_id(),
                1,
                &format!("invalid-{index}"),
                Change::Unchanged,
                invalid,
            ))
            .await
            .expect_err("invalid knowledge_change must fail");
        assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));

        let after = serde_json::to_value(store.load(spec.story_id()).await.expect("story should load"))
            .expect("after context serializes");
        assert_eq!(after, before);
    }
}
