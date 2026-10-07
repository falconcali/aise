use super::*;
use crate::core::{
    Change, IdempotencyKey, PackId, PackRef, PlayerId, RoleId, SemanticVersion, Sha256Digest, StoryCommit, StoryId,
    StoryInstanceSpec, StorySummary, Turn, TurnEvaluation, TurnNumber, TurnSegment, TurnStatus, WorldChange,
};
use crate::persistence::StoryStoreType;

fn story_id(value: &str) -> StoryId {
    StoryId::try_new(value).expect("test story id should be valid")
}

fn idempotency_key(value: &str) -> IdempotencyKey {
    IdempotencyKey::try_new(value).expect("test idempotency key should be valid")
}

fn story_spec(value: &str) -> StoryInstanceSpec {
    StoryInstanceSpec {
        story_id: story_id(value),
        pack_ref: PackRef {
            pack_id: PackId::try_new("pack").expect("test pack id should be valid"),
            version: SemanticVersion::try_new("1.0.0").expect("test version should be valid"),
            digest: Sha256Digest::try_new("digest").expect("test digest should be valid"),
        },
        cast: std::collections::BTreeMap::new(),
        player_id: PlayerId::try_new("player").expect("test player id should be valid"),
        player_role: RoleId::try_new("protagonist").expect("test role id should be valid"),
    }
}

fn commit(story_id: &StoryId, turn_number: u64, key: &str, summary: Change<StorySummary>) -> StoryCommit {
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
            world_change: WorldChange {},
            turn_evaluation: TurnEvaluation {},
            turn_status: TurnStatus::Accepted,
        },
        summary,
    }
}

#[tokio::test]
async fn creates_and_loads_story_context_and_info() {
    let store = StoryStoreMem::new();
    let spec = story_spec("story-1");
    let info = store.create(spec.clone()).await.expect("story creation should succeed");

    assert_eq!(info.story_id.as_str(), "story-1");
    assert!(matches!(info.life_cycle, crate::core::StoryLifeCycle::Active));

    let loaded = store.load(&spec.story_id).await.expect("story should load");
    assert_eq!(loaded.story_id, spec.story_id);
    assert_eq!(loaded.player, spec.player_role);
    assert_eq!(loaded.turn_number.value(), 0);
    assert!(loaded.summary.is_none());
    assert!(loaded.rencent_turns.is_empty());

    let loaded_info = store.get_info(&spec.story_id).await.expect("story info should load");
    assert_eq!(loaded_info.story_id, info.story_id);
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
    let first = commit(&spec.story_id, 1, "key-1", Change::Unchanged);

    let committed = store.commit(first.clone()).await.expect("commit should succeed");
    assert_eq!(committed.turn.turn_number.value(), 1);

    let replayed = store.commit(first).await.expect("replayed commit should succeed");
    assert_eq!(replayed.turn.turn_number.value(), 1);

    let found = store
        .find_committed(&spec.story_id, &idempotency_key("key-1"))
        .await
        .expect("idempotency lookup should succeed")
        .expect("committed turn should be found");
    assert_eq!(found.turn.turn_number.value(), 1);

    let loaded = store.load(&spec.story_id).await.expect("story should load");
    assert_eq!(loaded.turn_number.value(), 1);
    assert_eq!(loaded.rencent_turns.len(), 1);
}

#[tokio::test]
async fn removes_turns_covered_by_summary_before_window_trim() {
    let store = StoryStoreMem::with_config(StoryStoreConfig {
        store_type: StoryStoreType::Memory,
        max_recent_turns: 8,
    });
    let spec = story_spec("story-1");
    store.create(spec.clone()).await.expect("story creation should succeed");

    for turn_number in 1..=4 {
        let turn = commit(&spec.story_id, turn_number, &format!("key-{turn_number}"), Change::Unchanged);
        store.commit(turn).await.expect("turn commit should succeed");
    }

    let summary = StorySummary {
        text: "summary".to_owned(),
        covered_through: TurnNumber::new(3),
    };
    let turn = commit(&spec.story_id, 5, "key-5", Change::Replaced(summary));
    store.commit(turn).await.expect("summary commit should succeed");

    let loaded = store.load(&spec.story_id).await.expect("story should load");
    assert_eq!(
        loaded.summary.as_ref().expect("summary should exist").covered_through.value(),
        3
    );
    assert_eq!(
        loaded
            .rencent_turns
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

    let invalid_turn = commit(&spec.story_id, 2, "key-2", Change::Unchanged);
    let error = store.commit(invalid_turn).await.expect_err("turn conflict should fail");
    assert!(matches!(error, PersistenceError::ConstraintViolation { .. }));

    let future_summary = commit(
        &spec.story_id,
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
