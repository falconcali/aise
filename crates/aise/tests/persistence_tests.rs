use aise::config::{AssetLimitsConfig, ContextPreparationConfig, TurnContentLimitsConfig};
use aise::core::turn_contract::{IdempotencyKey, RequestDigest};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_validation::StateChange;
use aise::domain::StorySequence;
use aise::domain::asset::ids::{PlayerId, StoryRoleKey};
use aise::domain::ids::{StoryId, StoryRevision, TurnId};
use aise::domain::narrative::{StorySummary, StoryTurn};
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store, TurnCommitSpec};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_persist_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn valid_pack_json() -> String {
    serde_json::json!({
        "spec": "aise_story_v3",
        "spec_version": "3.0",
        "meta": {
            "pack_key": "demo",
            "title": "Demo",
            "author": "aise",
            "version": "0.1.0",
            "description": "demo pack"
        },
        "story": {
            "premise": "A quiet village.",
            "language": "zh-CN",
            "genre": ["adventure"],
            "themes": ["hope"],
            "style": {"tone": ["light"], "point_of_view": "third", "tense": "past"}
        },
        "character_assets": {},
        "roles": {
            "protagonist": {
                "role_label": "Protagonist",
                "narrative_function": "hero",
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": []
            }
        },
        "default_cast": {
            "protagonist": {"character_ref": "protagonist_card"}
        },
        "play": {
            "player_count": 1,
            "playable_role_keys": ["protagonist"]
        },
        "world_book": {
            "spec": "aise_world_v3",
            "spec_version": "3.0",
            "world_book_key": "demo_world",
            "meta": {"name": "Demo World", "version": "0.1.0"},
            "facts": {},
            "rumors": {}
        },
        "start": {
            "scene_key": "scene_1",
            "location_key": "village",
            "time": "morning",
            "description": "The village wakes.",
            "role_openings": {
                "protagonist": "You open your eyes."
            }
        },
        "narrative": {
            "entry_nodes": ["node_a"],
            "nodes": {
                "node_a": {
                    "title": "A",
                    "objective": "Wake up",
                    "activate_when": {"type": "story_started"},
                    "complete_when": {"type": "turn_reaches", "turn": 1},
                    "skip_when": null,
                    "effects": {"on_activate": [], "on_complete": []},
                    "terminal": false
                }
            },
            "edges": []
        },
        "assets": {}
    })
    .to_string()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits::from_config(
        &TurnContentLimitsConfig::default(),
        &ContextPreparationConfig::default(),
        &AssetLimitsConfig::default(),
    )
}

async fn create_instance(label: &str) -> (Arc<dyn Store>, StoryId, String) {
    let db = temp_db_path(label);
    let store: Arc<dyn Store> = SqliteStore::connect(&db).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(NativeAssetImporter::new(AssetLimitsConfig::default()), asset_store.clone());
    let info = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("pack import");
    let factory = StoryInstanceFactory::new(
        asset_store,
        store.clone(),
        StoryInstantiationLimits {
            max_roles: 16,
            max_facts: 128,
            max_rumors: 128,
            max_memories: 128,
            max_relationships: 64,
        },
    );
    let story = factory
        .create(CreateStoryInstanceSpec {
            pack_id: info.pack_id,
            player_id: PlayerId::from("player-1"),
            player_role_key: StoryRoleKey::from("protagonist"),
            player_character: None,
            created_at_ms: 1000,
        })
        .await
        .expect("create instance");
    (store, story.story_id, db)
}

fn commit_spec(story_id: &StoryId, base: StoryRevision, sequence: u64, key: &str, turn_id: &str) -> TurnCommitSpec {
    TurnCommitSpec {
        story_id: story_id.clone(),
        turn: StoryTurn {
            id: TurnId::try_new(turn_id).unwrap(),
            sequence: StorySequence::try_new(sequence).unwrap(),
            player_input: "input".into(),
            story_text: format!("story {turn_id}"),
            created_at: 1000 + sequence as i64,
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
        request_digest: RequestDigest::from_stored(format!("digest-{key}")),
        player_character_id: None,
        outbox: Vec::new(),
        llm_calls: Vec::new(),
    }
}

#[tokio::test]
async fn snapshot_is_revision_consistent() {
    let (store, story_id, db) = create_instance("snap_rev").await;
    let before = store.load_story_snapshot(&story_id, limits()).await.expect("load");
    assert_eq!(before.base_revision(), StoryRevision::new(0));
    store
        .commit_turn(&commit_spec(&story_id, before.base_revision(), 1, "k1", "t1"))
        .await
        .expect("commit");
    let after = store.load_story_snapshot(&story_id, limits()).await.expect("load after");
    assert_eq!(after.base_revision(), StoryRevision::new(1));
    assert_eq!(after.story_continuity().recent_segments().len(), 1);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn recent_segments_are_returned_in_sequence_order() {
    let (store, story_id, db) = create_instance("chrono").await;
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(0), 1, "k1", "t1"))
        .await
        .expect("t1");
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(1), 2, "k2", "t2"))
        .await
        .expect("t2");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load");
    let sequences: Vec<u64> = snapshot
        .story_continuity()
        .recent_segments()
        .iter()
        .map(|segment| segment.sequence.get())
        .collect();
    assert_eq!(sequences, vec![1, 2]);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_commit_assigns_next_story_sequence() {
    let (store, story_id, db) = create_instance("seq").await;
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(0), 1, "k1", "t1"))
        .await
        .expect("t1");
    let duplicate = store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(1), 1, "k2", "t2"))
        .await;
    assert!(duplicate.is_err(), "duplicate sequence must fail");
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn summary_change_updates_continuity_boundary() {
    let (store, story_id, db) = create_instance("summary").await;
    let mut spec = commit_spec(&story_id, StoryRevision::new(0), 1, "k1", "t1");
    spec.summary_change = StateChange::Replace(StorySummary {
        text: aise::domain::asset::validation::BoundedText::try_new("past", "summary", 1024).unwrap(),
        summarized_through: Some(StorySequence::try_new(1).unwrap()),
    });
    store.commit_turn(&spec).await.expect("commit");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load");
    assert_eq!(
        snapshot.story_continuity().summary().summarized_through,
        Some(StorySequence::try_new(1).unwrap())
    );
    let _ = std::fs::remove_file(&db);
}
