use aise::AiseError;
use aise::context::BaselineContextBuilder;
use aise::core::turn_budget::{TurnBudget, TurnBudgetLimits};
use aise::core::turn_context::TurnExecutionContext;
use aise::core::turn_contract::{
    IdempotencyKey, LlmUsageAggregate, RequestDigest, StoryRevision, TurnCancellation, TurnControl, TurnIdentity,
    TurnRequest,
};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_pipeline::TurnExecutionPipeline;
use aise::core::turn_trace::TraceRecorder;
use aise::core::turn_validation::StateChange;
use aise::domain::character::{CharacterState, InternalState};
use aise::domain::ids::{CharacterId, EventId, StoryId, TurnId};
use aise::domain::narrative::{EventKind, StoryEvent, StoryTurn};
use aise::persistence::{OutboxRecord, SqliteStore, Store, TurnCommit};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_p4_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits {
        max_recent_turns: 20,
        max_memories: 20,
    }
}

fn base_commit(story_id: &StoryId, base: StoryRevision, key: &str, digest: &str, turn_id: &str) -> TurnCommit {
    TurnCommit {
        story_id: story_id.clone(),
        turn: StoryTurn {
            id: TurnId::from(turn_id),
            player_input: "input".into(),
            story_text: format!("story {turn_id}"),
            summary_delta: None,
            created_at: 1000,
        },
        events: Vec::new(),
        characters: Vec::new(),
        world: StateChange::Unchanged,
        memory: Vec::new(),
        base_revision: base,
        idempotency_key: IdempotencyKey::try_new(key.to_string()).unwrap(),
        request_digest: RequestDigest::from_stored(digest.to_string()),
        player_character_id: None,
        outbox: Vec::new(),
        llm_usage: LlmUsageAggregate::default(),
    }
}

fn event(turn_id: &str, seq: u32, kind: EventKind) -> StoryEvent {
    StoryEvent {
        id: EventId::from(format!("{turn_id}#{seq}")),
        turn_id: TurnId::from(turn_id),
        seq,
        kind,
        payload: serde_json::json!({ "text": format!("event {seq}") }),
    }
}

fn outbox_record(story_id: &StoryId, turn_id: &str, id: &str) -> OutboxRecord {
    OutboxRecord {
        id: id.to_string(),
        story_id: story_id.clone(),
        turn_id: TurnId::from(turn_id),
        event_type: "story_event.action".into(),
        payload: serde_json::json!({ "text": id }),
        created_at: 1000,
    }
}

async fn open_pool(db: &str) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(db).unwrap();
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

async fn count_outbox(db: &str, story_id: &str) -> i64 {
    let pool = open_pool(db).await;
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE story_id = ?")
        .bind(story_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    row.0
}

async fn count_events(db: &str, story_id: &str) -> i64 {
    let pool = open_pool(db).await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM story_events WHERE turn_id IN (SELECT id FROM story_turns WHERE world_id = ?)",
    )
    .bind(story_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    row.0
}

async fn story_revision(db: &str, story_id: &str) -> i64 {
    let pool = open_pool(db).await;
    let row: (i64,) = sqlx::query_as("SELECT revision FROM stories WHERE id = ?")
        .bind(story_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    row.0
}

#[tokio::test]
async fn snapshot_is_revision_consistent() {
    let db = temp_db_path("snap_rev");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-snap");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    let before = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    assert_eq!(before.base_revision(), StoryRevision::new(0));

    store
        .commit_turn(&base_commit(&story_id, before.base_revision(), "k1", "d1", "t1"))
        .await
        .expect("commit turn");

    let after = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    assert_eq!(after.base_revision(), StoryRevision::new(1));
    assert_eq!(after.recent_turns().len(), 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn recent_turns_are_returned_in_chronological_order() {
    let db = temp_db_path("chrono");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-chrono");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    let mut t1 = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    t1.turn.created_at = 1001;
    store.commit_turn(&t1).await.expect("commit t1");
    let mut t2 = base_commit(&story_id, StoryRevision::new(1), "k2", "d2", "t2");
    t2.turn.created_at = 1002;
    store.commit_turn(&t2).await.expect("commit t2");
    let mut t3 = base_commit(&story_id, StoryRevision::new(2), "k3", "d3", "t3");
    t3.turn.created_at = 1003;
    store.commit_turn(&t3).await.expect("commit t3");

    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let created: Vec<i64> = snapshot.recent_turns().iter().map(|t| t.created_at).collect();
    assert_eq!(created, vec![1001, 1002, 1003]);
    assert_eq!(snapshot.recent_turns()[0].id, TurnId::from("t1"));
    assert_eq!(snapshot.recent_turns()[2].id, TurnId::from("t3"));

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn player_character_is_selected_by_stable_id() {
    let db = temp_db_path("player");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-player");

    store
        .create_story(&story_id, Some(&CharacterId::from("c-2")), 1000)
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
    turn.characters = vec![char_one, char_two];
    store.commit_turn(&turn).await.expect("commit characters");

    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
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
            TurnId::from("turn-p"),
            IdempotencyKey::try_new("key-p".to_string()).unwrap(),
            2000,
        )
        .unwrap(),
        TurnRequest::try_new("开始吧".to_string()).unwrap(),
        TurnBudget::new(TurnBudgetLimits {
            max_repair_rounds: 3,
            max_llm_calls: 8,
            max_input_tokens: 8_192,
            max_output_tokens: 2_048,
            max_total_tokens: 10_240,
            max_retrieved_items: 5,
        }),
        TurnControl::new(Instant::now() + Duration::from_secs(60), TurnCancellation::new()),
        TraceRecorder::new(),
    )
    .unwrap();
    ctx.complete_initialization().unwrap();
    BaselineContextBuilder::new(store.clone())
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
    let story_id = StoryId::from("story-rev");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1"))
        .await
        .expect("commit t1");

    let error = store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "k2", "d2", "t2"))
        .await
        .expect_err("stale base revision must conflict");
    assert!(matches!(error, AiseError::RevisionConflict));

    assert_eq!(story_revision(&db, "story-rev").await, 1);
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
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
    let story_id = StoryId::from("story-idem");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    let first = store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
        .await
        .expect("first commit");
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let replay = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-a", "t2"))
        .await
        .expect("replay commit must return original result");

    assert_eq!(first.turn_id, replay.turn_id);
    assert_eq!(first.story_revision, replay.story_revision);
    assert_eq!(first.story_text, replay.story_text);
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    assert_eq!(snapshot.recent_turns().len(), 1, "replay must not write a second turn");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn same_key_with_different_request_returns_conflict() {
    let db = temp_db_path("idem_conflict");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-idem-c");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    store
        .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
        .await
        .expect("first commit");
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let error = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-b", "t2"))
        .await
        .expect_err("different digest with same key must conflict");
    assert!(matches!(error, AiseError::IdempotencyConflict));
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    assert_eq!(snapshot.recent_turns().len(), 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn outbox_is_atomic_with_turn_commit() {
    let db = temp_db_path("outbox");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-outbox");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    let mut turn = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t1");
    turn.events = vec![event("t1", 0, EventKind::Action)];
    turn.outbox = vec![outbox_record(&story_id, "t1", "o1")];
    store.commit_turn(&turn).await.expect("commit turn with outbox");
    assert_eq!(count_outbox(&db, "story-outbox").await, 1);

    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let mut failing = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t1");
    failing.events = vec![event("t1", 1, EventKind::Action)];
    failing.outbox = vec![outbox_record(&story_id, "t1", "o2")];
    let error = store.commit_turn(&failing).await.expect_err("duplicate turn id must fail");
    assert!(matches!(error, AiseError::Store(_)));
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
    let story_id = StoryId::from("story-crash");
    {
        let store = SqliteStore::connect(&db).await.expect("connect store");
        store.create_story(&story_id, None, 1000).await.expect("create story");
        store
            .commit_turn(&base_commit(&story_id, StoryRevision::new(0), "key", "digest-a", "t1"))
            .await
            .expect("first commit");
    }

    let store = SqliteStore::connect(&db).await.expect("reconnect store after crash");
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let replayed = store
        .commit_turn(&base_commit(&story_id, snapshot.base_revision(), "key", "digest-a", "t2"))
        .await
        .expect("replay after crash returns original result");
    assert_eq!(replayed.turn_id, TurnId::from("t1"));
    assert_eq!(replayed.story_revision, StoryRevision::new(1));
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    assert_eq!(snapshot.recent_turns().len(), 1, "crash recovery must not write a second turn");
    assert_eq!(story_revision(&db, "story-crash").await, 1);

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn transaction_failure_persists_nothing() {
    let db = temp_db_path("rollback");
    let store = SqliteStore::connect(&db).await.expect("connect store");
    let story_id = StoryId::from("story-rollback");

    store.create_story(&story_id, None, 1000).await.expect("create story");
    let mut ok = base_commit(&story_id, StoryRevision::new(0), "k1", "d1", "t-dup");
    ok.events = vec![event("t-dup", 0, EventKind::Action)];
    ok.outbox = vec![outbox_record(&story_id, "t-dup", "o1")];
    store.commit_turn(&ok).await.expect("commit ok turn");

    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
    let mut failing = base_commit(&story_id, snapshot.base_revision(), "k2", "d2", "t-dup");
    failing.events = vec![event("t-dup", 1, EventKind::Action)];
    failing.outbox = vec![outbox_record(&story_id, "t-dup", "o2")];
    let error = store.commit_turn(&failing).await.expect_err("duplicate turn id must fail");
    assert!(matches!(error, AiseError::Store(_)));

    assert_eq!(story_revision(&db, "story-rollback").await, 1, "revision bump must roll back");
    assert_eq!(count_outbox(&db, "story-rollback").await, 1);
    assert_eq!(count_events(&db, "story-rollback").await, 1);
    let snapshot = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load snapshot")
        .expect("story exists");
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
