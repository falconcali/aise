use aise::config::{TurnConfig, TurnContentLimitsConfig};
use aise::context::BaselineContextBuilder;
use aise::core::turn_budget::TurnBudget;
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    FinishReason, IdempotencyKey, LlmCallId, LlmCallPurpose, LlmCallUsage, LlmCharge, RequestDigest, StoryRevision,
    TurnCancellation, TurnControl, TurnIdentity, TurnRequest, UsageAccuracy,
};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::core::turn_validation::{CharacterStateChange, MemoryStateChange, StateChange};
use aise::domain::character::{CharacterState, InternalState};
use aise::domain::ids::{CharacterId, EventId, MemoryId, StoryId, TurnId};
use aise::domain::memory::{MemoryEntry, MemoryKind};
use aise::domain::narrative::{EventKind, StoryEvent, StorySummary, StoryTurn};
use aise::domain::story_state::{ConstraintId, CurrentScene, StoryConfig, StoryConstraint, StoryCreateSpec};
use aise::persistence::{OutboxRecord, SqliteStore, Store, StoreError, TurnCommitSpec};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_p4_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits::from_config(&TurnContentLimitsConfig::default())
}

fn base_commit(story_id: &StoryId, base: StoryRevision, key: &str, digest: &str, turn_id: &str) -> TurnCommitSpec {
    TurnCommitSpec {
        story_id: story_id.clone(),
        turn: StoryTurn {
            id: TurnId::try_new(turn_id).unwrap(),
            player_input: "input".into(),
            story_text: format!("story {turn_id}"),
            created_at: 1000,
        },
        events: Vec::new(),
        character_changes: Vec::new(),
        world_change: StateChange::Unchanged,
        memory_changes: Vec::new(),
        scene_change: StateChange::Unchanged,
        constraint_change: StateChange::Unchanged,
        summary_change: StateChange::Unchanged,
        base_revision: base,
        idempotency_key: IdempotencyKey::try_new(key.to_string()).unwrap(),
        request_digest: RequestDigest::from_stored(digest.to_string()),
        player_character_id: None,
        outbox: Vec::new(),
        llm_calls: Vec::new(),
    }
}

fn event(turn_id: &str, seq: u32, kind: EventKind) -> StoryEvent {
    StoryEvent {
        id: EventId::from(format!("{turn_id}#{seq}")),
        turn_id: TurnId::try_new(turn_id).unwrap(),
        seq,
        kind,
        payload: serde_json::json!({ "text": format!("event {seq}") }),
    }
}

fn outbox_record(story_id: &StoryId, turn_id: &str, id: &str) -> OutboxRecord {
    OutboxRecord {
        id: id.to_string(),
        story_id: story_id.clone(),
        turn_id: TurnId::try_new(turn_id).unwrap(),
        event_type: "story_event.action".into(),
        payload: serde_json::json!({ "text": id }),
        created_at: 1000,
    }
}

async fn count_outbox(db: &str, story_id: &str) -> i64 {
    let store = SqliteStore::connect(db).await.expect("connect store");
    let pool = store.pool_for_tests();
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE story_id = ?")
        .bind(story_id)
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

async fn count_events(db: &str, story_id: &str) -> i64 {
    let store = SqliteStore::connect(db).await.expect("connect store");
    let pool = store.pool_for_tests();
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM story_events WHERE turn_id IN (SELECT id FROM story_turns WHERE world_id = ?)",
    )
    .bind(story_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

async fn story_revision(db: &str, story_id: &str) -> i64 {
    let store = SqliteStore::connect(db).await.expect("connect store");
    let pool = store.pool_for_tests();
    let row: (i64,) = sqlx::query_as("SELECT revision FROM stories WHERE id = ?")
        .bind(story_id)
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

fn create_spec(story_id: &StoryId, player_character_id: Option<&CharacterId>) -> StoryCreateSpec {
    StoryCreateSpec {
        story_id: story_id.clone(),
        story_instructions: String::new(),
        story_config: StoryConfig::default(),
        player_character_id: player_character_id.cloned(),
        initial_world: None,
        current_scene: CurrentScene { text: String::new() },
        story_summary: StorySummary { text: String::new() },
        active_constraints: Vec::new(),
        created_at_ms: 1000,
    }
}

async fn create_story(store: &dyn Store, story_id: &StoryId) {
    store.create_story(&create_spec(story_id, None)).await.expect("create story");
}

#[tokio::test]
async fn snapshot_is_revision_consistent() {
    let db = temp_db_path("snap_rev");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-snap").unwrap();

    create_story(&store, &story_id).await;
    let before = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(before.base_revision(), StoryRevision::new(0));

    store
        .commit_turn(&base_commit(&story_id, before.base_revision(), "k1", "d1", "t1"))
        .await
        .expect("commit turn");

    let after = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(after.base_revision(), StoryRevision::new(1));
    assert_eq!(after.recent_turns().len(), 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn recent_turns_are_returned_in_chronological_order() {
    let db = temp_db_path("chrono");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-chrono").unwrap();

    create_story(&store, &story_id).await;
    let mut t1 = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    t1.turn.created_at = 1001;
    store.commit_turn(&t1).await.expect("commit t1");
    let mut t2 = base_commit(&story_id, StoryRevision::new(1), "k2", "d2", "t2");
    t2.turn.created_at = 1002;
    store.commit_turn(&t2).await.expect("commit t2");
    let mut t3 = base_commit(&story_id, StoryRevision::new(2), "k3", "d3", "t3");
    t3.turn.created_at = 1003;
    store.commit_turn(&t3).await.expect("commit t3");

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let created: Vec<i64> = snapshot.recent_turns().iter().map(|t| t.created_at).collect();
    assert_eq!(created, vec![1001, 1002, 1003]);
    assert_eq!(snapshot.recent_turns()[0].id, TurnId::try_new("t1").unwrap());
    assert_eq!(snapshot.recent_turns()[2].id, TurnId::try_new("t3").unwrap());

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn player_character_is_selected_by_stable_id() {
    let db = temp_db_path("player");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-player").unwrap();

    store
        .create_story(&create_spec(&story_id, Some(&CharacterId::from("c-2"))))
        .await
        .expect("create story with player");
    let char_one = CharacterState {
        id: CharacterId::from("c-1"),
        name: "first".into(),
        bio: String::new(),
        internal_state: InternalState::default(),
    };
    let char_two = CharacterState {
        id: CharacterId::from("c-2"),
        name: "second".into(),
        bio: String::new(),
        internal_state: InternalState::default(),
    };
    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.character_changes = vec![
        aise::core::turn_validation::CharacterStateChange {
            character_id: char_one.id.clone(),
            new_state: char_one,
        },
        aise::core::turn_validation::CharacterStateChange {
            character_id: char_two.id.clone(),
            new_state: char_two,
        },
    ];
    store.commit_turn(&turn).await.expect("commit characters");

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.player_character_id(), Some(&CharacterId::from("c-2")));
    assert_eq!(snapshot.characters().len(), 2);
    let player = snapshot
        .characters()
        .iter()
        .find(|c| c.id == *snapshot.player_character_id().unwrap())
        .expect("player present");
    assert_eq!(player.name, "second");

    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            story_id.clone(),
            TurnId::try_new("turn-p").unwrap(),
            IdempotencyKey::try_new("key-p".to_string()).unwrap(),
            2000,
        )
        .unwrap(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        TurnBudget::from_config(&TurnConfig::default(), &aise::config::TurnContentLimitsConfig::default()).unwrap(),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    ctx.complete_initialization().unwrap();
    BaselineContextBuilder::new(store.clone(), TurnContentLimitsConfig::default())
        .execute(&mut ctx)
        .await
        .expect("baseline builder");
    let baseline = ctx.baseline().expect("baseline set");
    assert_eq!(baseline.player_character.as_ref().expect("player").id, CharacterId::from("c-2"));
    assert_eq!(baseline.relevant_characters.len(), 2);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn revision_conflict_rolls_back_every_change() {
    let db = temp_db_path("rev_conflict");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-rev").unwrap();

    create_story(&store, &story_id).await;
    store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1"))
        .await
        .expect("commit t1");

    let error = store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "k2", "d2", "t2"))
        .await
        .expect_err("stale base revision must conflict");
    assert!(matches!(error, StoreError::RevisionConflict));

    assert_eq!(story_revision(&db, "story-rev").await, 1);
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 1);
    assert!(
        store
            .find_committed_turn(&story_id, &IdempotencyKey::try_new("k2".to_string()).unwrap())
            .await
            .expect("lookup")
            .is_none()
    );

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn same_idempotency_key_returns_original_result() {
    let db = temp_db_path("idem_same");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-idem").unwrap();

    create_story(&store, &story_id).await;
    let first = store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
        .await
        .expect("first commit");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let replay = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-a", "t2"))
        .await
        .expect("replay commit must return original result");

    assert_eq!(first.turn_id, replay.turn_id);
    assert_eq!(first.story_revision, replay.story_revision);
    assert_eq!(first.story_text, replay.story_text);
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 1, "replay must not write a second turn");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn same_key_with_different_request_returns_conflict() {
    let db = temp_db_path("idem_conflict");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-idem-c").unwrap();

    create_story(&store, &story_id).await;
    store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
        .await
        .expect("first commit");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let error = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-b", "t2"))
        .await
        .expect_err("different digest with same key must conflict");
    assert!(matches!(error, StoreError::IdempotencyConflict));
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn outbox_is_atomic_with_turn_commit() {
    let db = temp_db_path("outbox");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-outbox").unwrap();

    create_story(&store, &story_id).await;
    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.events = vec![event("t1", 0, EventKind::Action)];
    turn.outbox = vec![outbox_record(&story_id, "t1", "o1")];
    store.commit_turn(&turn).await.expect("commit turn with outbox");
    assert_eq!(count_outbox(&db, "story-outbox").await, 1);

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let mut failing = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t1");
    failing.events = vec![event("t1", 1, EventKind::Action)];
    failing.outbox = vec![outbox_record(&story_id, "t1", "o2")];
    let error = store.commit_turn(&failing).await.expect_err("duplicate turn id must fail");
    assert!(matches!(error, StoreError::ConstraintViolation { .. }));
    assert_eq!(
        count_outbox(&db, "story-outbox").await,
        1,
        "failed commit must not leak outbox rows"
    );
    assert_eq!(count_events(&db, "story-outbox").await, 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn crash_recovery_returns_original_result_without_second_write() {
    let db = temp_db_path("crash");
    let story_id = StoryId::try_new("story-crash").unwrap();
    {
        let store = SqliteStore::connect(&db).await.expect("connect store");
        create_story(&store, &story_id).await;
        store
            .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
            .await
            .expect("first commit");
    }

    let store = SqliteStore::connect(&db).await.expect("reconnect store after crash");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let replayed = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-a", "t2"))
        .await
        .expect("replay after crash returns original result");
    assert_eq!(replayed.turn_id, TurnId::try_new("t1").unwrap());
    assert_eq!(replayed.story_revision, StoryRevision::new(1));
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 1, "crash recovery must not write a second turn");
    assert_eq!(story_revision(&db, "story-crash").await, 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn transaction_failure_persists_nothing() {
    let db = temp_db_path("rollback");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-rollback").unwrap();

    create_story(&store, &story_id).await;
    let mut ok = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t-dup");
    ok.events = vec![event("t-dup", 0, EventKind::Action)];
    ok.outbox = vec![outbox_record(&story_id, "t-dup", "o1")];
    store.commit_turn(&ok).await.expect("commit ok turn");

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let mut failing = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t-dup");
    failing.events = vec![event("t-dup", 1, EventKind::Action)];
    failing.outbox = vec![outbox_record(&story_id, "t-dup", "o2")];
    let error = store.commit_turn(&failing).await.expect_err("duplicate turn id must fail");
    assert!(matches!(error, StoreError::ConstraintViolation { .. }));

    assert_eq!(story_revision(&db, "story-rollback").await, 1, "revision bump must roll back");
    assert_eq!(count_outbox(&db, "story-rollback").await, 1);
    assert_eq!(count_events(&db, "story-rollback").await, 1);
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.recent_turns().len(), 1);
    assert!(
        store
            .find_committed_turn(&story_id, &IdempotencyKey::try_new("k2".to_string()).unwrap())
            .await
            .expect("lookup")
            .is_none()
    );

    let _ = std::fs::remove_file(&db);
}

fn call_usage(seq: u64) -> LlmCallUsage {
    LlmCallUsage {
        call_id: LlmCallId::new(),
        purpose: LlmCallPurpose::StoryGeneration,
        provider: "mock".into(),
        model: "mock-model".into(),
        input_tokens: 100 + seq,
        cached_input_tokens: Some(seq),
        output_tokens: 50 + seq,
        total_tokens: 150 + 2 * seq,
        accuracy: UsageAccuracy::Exact,
        pricing_version: Some("price-v1".into()),
        charge: Some(LlmCharge {
            provider: "mock".into(),
            model: "mock-model".into(),
            input_tokens: 100 + seq,
            cached_input_tokens: seq,
            output_tokens: 50 + seq,
            amount_minor: 10 + seq as i64,
            price_version: "price-v1".into(),
        }),
        finish_reason: Some(FinishReason::Stop),
    }
}

#[tokio::test]
async fn committed_result_persists_bounded_per_call_ledger() {
    let db = temp_db_path("ledger");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-ledger").unwrap();
    create_story(&store, &story_id).await;

    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.llm_calls = (0..3).map(call_usage).collect();
    let result = store.commit_turn(&turn).await.expect("commit turn");

    let max_calls = TurnConfig::default().max_llm_calls;
    assert!(
        result.llm_calls.len() as u32 <= max_calls,
        "per-call ledger stays within the max_llm_calls bound"
    );
    assert_eq!(result.llm_calls.len(), 3);
    assert_eq!(result.llm_usage.llm_calls, 3);
    assert_eq!(result.llm_usage.input_tokens, 100 + 101 + 102);
    assert_eq!(result.llm_usage.output_tokens, 50 + 51 + 52);
    assert_eq!(result.llm_usage.total_tokens, 150 + 152 + 154);

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let mut second = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t2");
    second.llm_calls = (0..4).map(call_usage).collect();
    let second_result = store.commit_turn(&second).await.expect("commit second turn");
    assert_eq!(second_result.llm_calls.len(), 4);
    assert_eq!(second_result.llm_usage.llm_calls, 4);

    let recovered = store
        .find_committed_turn(&story_id, &IdempotencyKey::try_new("k1".to_string()).unwrap())
        .await
        .expect("lookup committed turn")
        .expect("k1 recovered");
    assert_eq!(recovered.result.llm_calls.len(), 3, "ledger survives persistence");
    assert_eq!(recovered.result.llm_usage, result.llm_usage);
    assert_eq!(recovered.result.llm_calls[0].charge.as_ref().expect("charge").amount_minor, 10);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn idempotency_replay_preserves_usage_and_charge() {
    let db = temp_db_path("ledger_replay");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-replay").unwrap();
    create_story(&store, &story_id).await;

    let mut first = base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1");
    first.llm_calls = (0..2).map(call_usage).collect();
    let first_result = store.commit_turn(&first).await.expect("first commit");
    assert_eq!(first_result.llm_calls.len(), 2);

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let mut replay = base_commit(&story_id, snapshot.base_revision(), "key", "digest-a", "t2");
    replay.llm_calls = (0..2).map(call_usage).collect();
    let replayed = store.commit_turn(&replay).await.expect("idempotent replay");

    assert_eq!(replayed.turn_id, first_result.turn_id, "replay returns the original turn");
    assert_eq!(
        serde_json::to_string(&replayed).expect("serialize replay"),
        serde_json::to_string(&first_result).expect("serialize original"),
        "idempotency replay returns byte-equivalent accounting data"
    );
    assert_eq!(replayed.llm_calls.len(), 2);
    assert_eq!(replayed.llm_usage, first_result.llm_usage);
    assert_eq!(
        replayed.llm_calls[1].charge.as_ref().map(|c| c.amount_minor),
        first_result.llm_calls[1].charge.as_ref().map(|c| c.amount_minor)
    );

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn commit_atomically_updates_scene_summary_constraints_and_revision() {
    let db = temp_db_path("auth_state");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-auth").unwrap();
    store
        .create_story(&StoryCreateSpec {
            story_id: story_id.clone(),
            story_instructions: "instructions".into(),
            story_config: StoryConfig::default(),
            player_character_id: None,
            initial_world: None,
            current_scene: CurrentScene {
                text: "scene before".into(),
            },
            story_summary: StorySummary {
                text: "summary before".into(),
            },
            active_constraints: Vec::new(),
            created_at_ms: 1000,
        })
        .await
        .expect("create story");

    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.scene_change = StateChange::Replace(CurrentScene {
        text: "scene after".into(),
    });
    turn.constraint_change = StateChange::Replace(vec![StoryConstraint {
        id: ConstraintId::try_new("c1".to_string()).unwrap(),
        text: "no spoilers".into(),
    }]);
    turn.summary_change = StateChange::Replace(StorySummary {
        text: "summary after".into(),
    });
    let result = store.commit_turn(&turn).await.expect("commit turn");

    assert_eq!(result.story_revision, StoryRevision::new(1));
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.base_revision(), StoryRevision::new(1));
    assert_eq!(snapshot.current_scene().text, "scene after");
    assert_eq!(snapshot.story_summary().text, "summary after");
    assert_eq!(snapshot.active_constraints().len(), 1);
    assert_eq!(snapshot.active_constraints()[0].text, "no spoilers");
    assert_eq!(
        snapshot.story_instructions(),
        "instructions",
        "instructions are never rewritten by a turn"
    );

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn authoritative_state_rolls_back_on_commit_failure() {
    let db = temp_db_path("auth_rollback");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-auth-rb").unwrap();
    store
        .create_story(&StoryCreateSpec {
            story_id: story_id.clone(),
            story_instructions: String::new(),
            story_config: StoryConfig::default(),
            player_character_id: None,
            initial_world: None,
            current_scene: CurrentScene {
                text: "scene v1".into(),
            },
            story_summary: StorySummary {
                text: "summary v1".into(),
            },
            active_constraints: Vec::new(),
            created_at_ms: 1000,
        })
        .await
        .expect("create story");

    let mut ok = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t-dup");
    ok.scene_change = StateChange::Replace(CurrentScene {
        text: "scene v1".into(),
    });
    ok.summary_change = StateChange::Replace(StorySummary {
        text: "summary v1".into(),
    });
    ok.events = vec![event("t-dup", 0, EventKind::Action)];
    store.commit_turn(&ok).await.expect("commit ok turn");

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    let mut failing = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t-dup");
    failing.scene_change = StateChange::Replace(CurrentScene {
        text: "scene v2".into(),
    });
    failing.constraint_change = StateChange::Replace(vec![StoryConstraint {
        id: ConstraintId::try_new("c2".to_string()).unwrap(),
        text: "must not persist".into(),
    }]);
    failing.summary_change = StateChange::Replace(StorySummary {
        text: "summary v2".into(),
    });
    failing.outbox = vec![outbox_record(&story_id, "t-dup", "o2")];
    let error = store.commit_turn(&failing).await.expect_err("duplicate turn id must fail");
    assert!(matches!(error, StoreError::ConstraintViolation { .. }));

    assert_eq!(story_revision(&db, "story-auth-rb").await, 1, "revision bump must roll back");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.current_scene().text, "scene v1", "scene change must roll back");
    assert_eq!(snapshot.story_summary().text, "summary v1", "summary change must roll back");
    assert_eq!(snapshot.active_constraints().len(), 0, "constraint change must roll back");
    assert_eq!(count_outbox(&db, "story-auth-rb").await, 0, "outbox must roll back");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn baseline_uses_authoritative_scene_summary_and_constraints() {
    let db = temp_db_path("baseline_auth");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-baseline-auth").unwrap();
    store
        .create_story(&StoryCreateSpec {
            story_id: story_id.clone(),
            story_instructions: "instr".into(),
            story_config: StoryConfig {
                style: Some("dark".into()),
                point_of_view: None,
                tense: None,
            },
            player_character_id: None,
            initial_world: None,
            current_scene: CurrentScene {
                text: "authoritative scene".into(),
            },
            story_summary: StorySummary {
                text: "authoritative summary".into(),
            },
            active_constraints: vec![StoryConstraint {
                id: ConstraintId::try_new("c1".to_string()).unwrap(),
                text: "authoritative constraint".into(),
            }],
            created_at_ms: 1000,
        })
        .await
        .expect("create story");

    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.scene_change = StateChange::Replace(CurrentScene {
        text: "scene derived from latest turn".into(),
    });
    turn.summary_change = StateChange::Replace(StorySummary {
        text: "summary derived from latest turn".into(),
    });
    store.commit_turn(&turn).await.expect("commit turn");

    let mut ctx = TurnExecutionContext::new(
        TurnIdentity::new(
            story_id.clone(),
            TurnId::try_new("turn-b").unwrap(),
            IdempotencyKey::try_new("key-b".to_string()).unwrap(),
            2000,
        )
        .unwrap(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        TurnBudget::from_config(&TurnConfig::default(), &aise::config::TurnContentLimitsConfig::default()).unwrap(),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    ctx.complete_initialization().unwrap();
    BaselineContextBuilder::new(store.clone(), TurnContentLimitsConfig::default())
        .execute(&mut ctx)
        .await
        .expect("baseline builder");
    let baseline = ctx.baseline().expect("baseline set");
    assert_eq!(
        baseline.current_scene.as_deref(),
        Some("scene derived from latest turn"),
        "baseline scene is the authoritative committed scene, not derived from latest turn text"
    );
    assert_eq!(baseline.story_summary, "summary derived from latest turn");
    assert_eq!(
        baseline.active_constraints.len(),
        1,
        "baseline constraints come from the authoritative constraints table"
    );
    assert_eq!(baseline.active_constraints[0], "authoritative constraint");
    assert_eq!(baseline.story_instructions, "instr");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn snapshot_is_revision_consistent_under_concurrent_commit() {
    let db = temp_db_path("snap_concurrent");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-snap-conc").unwrap();
    store
        .create_story(&StoryCreateSpec {
            story_id: story_id.clone(),
            story_instructions: String::new(),
            story_config: StoryConfig::default(),
            player_character_id: None,
            initial_world: None,
            current_scene: CurrentScene { text: "s0".into() },
            story_summary: StorySummary { text: "sum0".into() },
            active_constraints: Vec::new(),
            created_at_ms: 1000,
        })
        .await
        .expect("create story");

    let reader_store = store.clone();
    let reader_story = story_id.clone();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_stop = stop.clone();
    let reader = tokio::spawn(async move {
        while !reader_stop.load(std::sync::atomic::Ordering::Relaxed) {
            let snapshot = reader_store
                .load_story_snapshot(&reader_story, limits())
                .await
                .expect("load snapshot");
            let revision = snapshot.base_revision().get() as usize;
            assert_eq!(
                snapshot.recent_turns().len(),
                revision,
                "turn history and revision stay aligned"
            );
            assert_eq!(
                snapshot.current_scene().text,
                format!("s{revision}"),
                "scene field matches the same base revision"
            );
            assert_eq!(
                snapshot.story_summary().text,
                format!("sum{revision}"),
                "summary field matches the same base revision"
            );
        }
    });

    for index in 0..8 {
        let mut turn = base_commit(
            &story_id,
            StoryRevision::new(index as u64),
            &format!("k{index}"),
            &format!("d{index}"),
            &format!("t{index}"),
        );
        turn.scene_change = StateChange::Replace(CurrentScene {
            text: format!("s{}", index + 1),
        });
        turn.summary_change = StateChange::Replace(StorySummary {
            text: format!("sum{}", index + 1),
        });
        store.commit_turn(&turn).await.expect("commit turn");
    }

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    reader.await.expect("reader finished");

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(snapshot.base_revision(), StoryRevision::new(8));
    assert_eq!(snapshot.recent_turns().len(), 8);
    assert_eq!(snapshot.current_scene().text, "s8");
    assert_eq!(snapshot.story_summary().text, "sum8");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn commit_turn_never_creates_missing_story() {
    let db = temp_db_path("no_auto_create_store");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-ghost").unwrap();

    let error = store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1"))
        .await
        .expect_err("commit for a missing story must fail");
    assert!(matches!(error, StoreError::RevisionConflict));
    assert!(
        store.get_story(&story_id).await.expect("get story").is_none(),
        "commit must never implicitly create a story row"
    );

    let _ = std::fs::remove_file(&db);
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("assets/persistence/mig");

#[tokio::test]
async fn migration_upgrades_existing_database_without_losing_committed_turns() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    let db = temp_db_path("upgrade");
    let pre_0005: Vec<&sqlx::migrate::Migration> = MIGRATOR.migrations.iter().filter(|m| m.version <= 4).collect();
    assert_eq!(pre_0005.len(), 4, "migrations 0001-0004 exist");

    let options = SqliteConnectOptions::from_str(&db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("connect pre-upgrade pool");

    for migration in &pre_0005 {
        sqlx::query(&migration.sql)
            .execute(&pool)
            .await
            .expect("apply pre-0005 migration");
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
         version BIGINT PRIMARY KEY, description TEXT NOT NULL, \
         installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, \
         success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)",
    )
    .execute(&pool)
    .await
    .expect("create migrations table");
    for migration in &pre_0005 {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (?, ?, TRUE, ?, -1)",
        )
        .bind(migration.version)
        .bind(&*migration.description)
        .bind(&*migration.checksum)
        .execute(&pool)
        .await
        .expect("record pre-0005 migration");
    }

    sqlx::query(
        "INSERT INTO stories (id, revision, player_character_id, created_at) VALUES ('story-old', 1, NULL, 1000)",
    )
    .execute(&pool)
    .await
    .expect("insert pre-0005 story");
    sqlx::query(
        "INSERT INTO story_turns (id, world_id, player_input, story_text, summary_delta, status, created_at, \
         idempotency_key, request_digest, base_revision, committed_revision, result_json) \
         VALUES ('turn-old', 'story-old', 'input', 'old story text', NULL, 'ok', 1000, 'key-old', 'digest-old', 0, 1, \
         '{\"turn_id\":\"turn-old\",\"story_revision\":1,\"story_text\":\"old story text\",\"llm_usage\":{\"llm_calls\":0,\"input_tokens\":0,\"output_tokens\":0,\"total_tokens\":0},\"llm_calls\":[]}')",
    )
    .execute(&pool)
    .await
    .expect("insert pre-0005 committed turn");

    MIGRATOR.run(&pool).await.expect("run 0005 upgrade migration");

    let store = match SqliteStore::connect(&db).await {
        Ok(store) => store,
        Err(error) => panic!("connect upgraded store failed: {error:?}"),
    };
    let snapshot = store
        .load_story_snapshot(&StoryId::try_new("story-old").unwrap(), limits())
        .await
        .expect("load upgraded snapshot");
    assert_eq!(snapshot.base_revision(), StoryRevision::new(1));
    assert_eq!(snapshot.recent_turns().len(), 1, "committed turn survives the upgrade");
    assert_eq!(snapshot.recent_turns()[0].story_text, "old story text");
    assert_eq!(snapshot.current_scene().text, "", "upgraded rows get explicit empty state");
    assert_eq!(snapshot.story_summary().text, "");
    assert_eq!(snapshot.active_constraints().len(), 0);
    let recovered = store
        .find_committed_turn(
            &StoryId::try_new("story-old").unwrap(),
            &IdempotencyKey::try_new("key-old".to_string()).unwrap(),
        )
        .await
        .expect("lookup committed turn after upgrade");
    assert!(recovered.is_some(), "recovery API still serves pre-upgrade committed turns");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn snapshot_query_limits_before_decode() {
    let db = temp_db_path("snapshot_limits");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::try_new("story-limits").unwrap();
    let player = CharacterId::from("c-player");
    store
        .create_story(&create_spec(&story_id, Some(&player)))
        .await
        .expect("create story");

    let mut revision = 0u64;
    for turn in 0..25 {
        let mut commit = base_commit(
            &story_id,
            StoryRevision::new(revision),
            &format!("key-{turn}"),
            &format!("digest-{turn}"),
            &format!("turn-{turn}"),
        );
        commit.character_changes = vec![
            CharacterStateChange {
                character_id: player.clone(),
                new_state: CharacterState {
                    id: player.clone(),
                    name: "player".into(),
                    bio: String::new(),
                    internal_state: InternalState::default(),
                },
            },
            CharacterStateChange {
                character_id: CharacterId::from(format!("c-{turn}")),
                new_state: CharacterState {
                    id: CharacterId::from(format!("c-{turn}")),
                    name: format!("char {turn}"),
                    bio: String::new(),
                    internal_state: InternalState::default(),
                },
            },
        ];
        commit.memory_changes = (0..2)
            .map(|index| MemoryStateChange {
                character_id: player.clone(),
                entry: MemoryEntry {
                    id: MemoryId::from(format!("m-{turn}-{index}")),
                    owner: player.clone(),
                    kind: MemoryKind::Observed,
                    content: format!("memory {turn}-{index}"),
                    created_at: 1000 + turn as i64,
                },
            })
            .collect();
        let result = match store.commit_turn(&commit).await {
            Ok(result) => result,
            Err(error) => panic!("commit turn {turn} failed: {error:?}"),
        };
        revision = result.story_revision.get();
    }

    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load snapshot");
    assert_eq!(
        snapshot.characters().len(),
        limits().max_characters(),
        "characters query must apply the LIMIT before decoding rows"
    );
    assert_eq!(
        snapshot.player_memories().len(),
        limits().max_memories(),
        "memory query must apply the LIMIT before decoding rows"
    );
    assert_eq!(
        snapshot.recent_turns().len(),
        limits().max_recent_turns(),
        "turns query must apply the LIMIT before decoding rows"
    );

    let _ = std::fs::remove_file(&db);
}
