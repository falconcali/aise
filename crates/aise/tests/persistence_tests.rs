use aise::config::{AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, TurnContentLimitsConfig};
use aise::domain::StorySequence;
use aise::domain::asset::ids::PlayerId;
use aise::domain::ids::{RoleId, StoryId, StoryRevision, TurnId};
use aise::domain::narrative::StoryTurn;
use aise::domain::turn::{SnapshotLimits, StoryCandidateVersion, ValidatedNarrativeResolution};
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store, TurnCommitSpec};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use aise::turn::turn_contract::{IdempotencyKey, RequestDigest};
use aise::turn::turn_validation::{StateChange, ValidatedChangeSet, ValidatedChangeSetParts};
use std::collections::BTreeMap;
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
        "spec": "aise_story_v4",
        "spec_version": "4.0",
        "meta": {
            "pack_key": "demo",
            "title": "Demo",
            "author": "aise",
            "version": "0.1.0",
            "description": "demo pack",
            "tags": [],
            "cover_asset": null
        },
        "story": {
            "premise": "A quiet village.",
            "language": "zh-CN",
            "genre": ["adventure"],
            "themes": ["hope"],
            "style": {"tone": ["light"], "point_of_view": "third", "tense": "past"}
        },
        "roles": {
            "protagonist": {
                "role_label": "Protagonist",
                "narrative_function": "hero",
                "default_profile": {
                    "name": "Hero",
                    "dialogue_examples": []
                },
                "background": null,
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": []
            }
        },
        "play": {
            "player_count": 1,
            "playable_role_ids": ["protagonist"]
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
            "opening": "You open your eyes."
        },
        "narrative": {
            "entry_nodes": ["node_a"],
            "nodes": {
                "node_a": {
                    "title": "A",
                    "dramatic_focus": "Wake up",
                    "activate_when": {"type": "story_started"},
                    "complete_when": {"type": "turn_reaches", "turn": 1},
                    "skip_when": null,
                    "effects": {"on_activate": [], "on_complete": []},
                    "terminal": false
                }
            },
            "edges": []
        },
        "constraints": {},
        "assets": {}
    })
    .to_string()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits::from_config(
        &TurnContentLimitsConfig::default(),
        &ContextPreparationConfig::default(),
        &AssetLimitsConfig::default(),
        &NarrativeConfig::default(),
    )
}

async fn create_instance(label: &str) -> (Arc<dyn Store>, StoryId, String) {
    let db = temp_db_path(label);
    let store: Arc<dyn Store> = SqliteStore::connect(&db).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(
        NativeAssetImporter::new(AssetLimitsConfig::default(), NarrativeConfig::default()),
        asset_store.clone(),
    );
    let info = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("pack import");
    let factory = StoryInstanceFactory::new(
        asset_store,
        store.clone(),
        StoryInstantiationLimits {
            max_roles: 16,
            max_role_bytes: 131_072,
            max_facts: 128,
            max_rumors: 128,
            max_memories: 128,
            max_relationships: 64,
            max_opening_bytes: 8192,
        },
        NarrativeConfig::default().as_limits(),
    );
    let story = factory
        .create(CreateStoryInstanceSpec {
            pack_id: info.pack_id,
            player_id: PlayerId::from("player-1"),
            player_role_id: RoleId::try_new("protagonist").unwrap(),
            role_profile_selections: BTreeMap::new(),
            created_at_ms: 1000,
        })
        .await
        .expect("create instance");
    (store, story.story_id, db)
}

fn commit_spec(
    story_id: &StoryId,
    base: StoryRevision,
    expected_graph_revision: u64,
    sequence: u64,
    key: &str,
    turn_id: &str,
) -> TurnCommitSpec {
    let story_text = format!("story {turn_id}");
    TurnCommitSpec {
        story_id: story_id.clone(),
        turn: StoryTurn {
            id: TurnId::try_new(turn_id).unwrap(),
            sequence: StorySequence::try_new(sequence).unwrap(),
            player_input: "input".into(),
            story_text: story_text.clone(),
            created_at: 1000 + sequence as i64,
        },
        base_revision: base,
        expected_graph_revision,
        changes: ValidatedChangeSet::new(ValidatedChangeSetParts {
            story_text: aise::domain::asset::validation::BoundedText::try_new(story_text, "story_text", 1024).unwrap(),
            role_changes: Vec::new(),
            relationship_changes: Vec::new(),
            knowledge_mutations: Vec::new(),
            narrative_events: Vec::new(),
            narrative_resolution: ValidatedNarrativeResolution {
                candidate_version: StoryCandidateVersion {
                    content_digest: aise::domain::asset::ids::Sha256Digest::from_bytes([0u8; 32]),
                    repair_attempt: 0,
                },
                transitions: Vec::new(),
                condition_results: BTreeMap::new(),
                pending_effects: Vec::new(),
                next_graph_revision: 1,
            },
            constraint_change: StateChange::Unchanged,
        })
        .unwrap(),
        idempotency_key: IdempotencyKey::try_new(key.to_string()).unwrap(),
        request_digest: RequestDigest::from_stored(format!("digest-{key}")),
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
        .commit_turn(&commit_spec(
            &story_id,
            before.base_revision(),
            before.graph_revision(),
            1,
            "k1",
            "t1",
        ))
        .await
        .expect("commit");
    let after = store.load_story_snapshot(&story_id, limits()).await.expect("load after");
    assert_eq!(after.base_revision(), StoryRevision::new(1));
    assert_eq!(after.story_continuity().recent_segments().len(), 2);
    assert_eq!(after.story_continuity().recent_segments()[1].sequence.get(), 2);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn recent_segments_are_returned_in_sequence_order() {
    let (store, story_id, db) = create_instance("chrono").await;
    let graph_revision = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load")
        .graph_revision();
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(0), graph_revision, 1, "k1", "t1"))
        .await
        .expect("t1");
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(1), graph_revision, 2, "k2", "t2"))
        .await
        .expect("t2");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load");
    let sequences: Vec<u64> = snapshot
        .story_continuity()
        .recent_segments()
        .iter()
        .map(|segment| segment.sequence.get())
        .collect();
    assert_eq!(sequences, vec![1, 2, 3]);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn turn_commit_assigns_next_story_sequence() {
    let (store, story_id, db) = create_instance("seq").await;
    let graph_revision = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load")
        .graph_revision();
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(0), graph_revision, 1, "k1", "t1"))
        .await
        .expect("t1");
    let second = store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(1), graph_revision, 1, "k2", "t2"))
        .await;
    assert!(second.is_ok(), "store assigns sequence independently of the proposal");
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn commit_turn_does_not_write_current_scene() {
    let (store, story_id, db) = create_instance("no_scene_write").await;
    let graph_revision = store
        .load_story_snapshot(&story_id, limits())
        .await
        .expect("load")
        .graph_revision();
    store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(0), graph_revision, 1, "k1", "t1"))
        .await
        .expect("commit should succeed without any current_scene column");

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db)
        .await
        .expect("reconnect");
    let columns: Vec<String> = sqlx::query("PRAGMA table_info(stories)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| sqlx::Row::get::<String, _>(&row, "name"))
        .collect();
    assert!(
        !columns.iter().any(|name| name == "current_scene"),
        "commit_turn must not depend on a stories.current_scene column"
    );
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}
