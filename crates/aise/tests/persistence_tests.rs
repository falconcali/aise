use aise::config::{AssetLimitsConfig, ContextPreparationConfig, TurnContentLimitsConfig};
use aise::core::turn_contract::{IdempotencyKey, RequestDigest};
use aise::core::turn_data::SnapshotLimits;
use aise::core::turn_validation::{StateChange, ValidatedChangeSet, ValidatedChangeSetParts};
use aise::domain::StorySequence;
use aise::domain::asset::ids::{PlayerId, StoryRoleKey};
use aise::domain::ids::{StoryId, StoryRevision, TurnId};
use aise::domain::narrative::{StorySummary, StoryTurn};
use aise::domain::story_instance::snapshot::NarrativeConditionStateView;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store, TurnCommitSpec};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::collections::{BTreeMap, BTreeSet};
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
        "character_assets": {
            "protagonist_card": {
                "spec": "aise_char_v3", "spec_version": "3.0", "character_key": "protagonist_card",
                "meta": {"name": "Hero", "version": "0.1.0"},
                "profile": {"description": "Hero", "personality": [], "values": [], "speaking_style": {"register": "neutral", "verbosity": "medium"}}
            }
        },
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
        expected_graph_revision: 0,
        changes: ValidatedChangeSet::new(ValidatedChangeSetParts {
            story_text: aise::domain::asset::validation::BoundedText::try_new(story_text, "story_text", 1024).unwrap(),
            events: Vec::new(),
            character_changes: Vec::new(),
            relationship_changes: Vec::new(),
            knowledge_additions: Vec::new(),
            current_perceptions: Vec::new(),
            scene_change: StateChange::Unchanged,
            narrative_changes: Vec::new(),
            condition_state: NarrativeConditionStateView {
                occurred_event_keys: BTreeSet::new(),
                player_action_event_keys: BTreeSet::new(),
                fact_values: BTreeMap::new(),
            },
            constraint_change: StateChange::Unchanged,
            summary_change: StateChange::Unchanged,
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
    let second = store
        .commit_turn(&commit_spec(&story_id, StoryRevision::new(1), 1, "k2", "t2"))
        .await;
    assert!(second.is_ok(), "store assigns sequence independently of the proposal");
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn summary_change_updates_continuity_boundary() {
    let (store, story_id, db) = create_instance("summary").await;
    let mut spec = commit_spec(&story_id, StoryRevision::new(0), 1, "k1", "t1");
    spec.changes = ValidatedChangeSet::new(ValidatedChangeSetParts {
        story_text: aise::domain::asset::validation::BoundedText::try_new("story t1", "story_text", 1024).unwrap(),
        events: Vec::new(),
        character_changes: Vec::new(),
        relationship_changes: Vec::new(),
        knowledge_additions: Vec::new(),
        current_perceptions: Vec::new(),
        scene_change: StateChange::Unchanged,
        narrative_changes: Vec::new(),
        condition_state: NarrativeConditionStateView {
            occurred_event_keys: BTreeSet::new(),
            player_action_event_keys: BTreeSet::new(),
            fact_values: BTreeMap::new(),
        },
        constraint_change: StateChange::Unchanged,
        summary_change: StateChange::Replace(StorySummary {
            text: aise::domain::asset::validation::BoundedText::try_new("past", "summary", 1024).unwrap(),
            summarized_through: Some(StorySequence::try_new(1).unwrap()),
        }),
    })
    .unwrap();
    store.commit_turn(&spec).await.expect("commit");
    let snapshot = store.load_story_snapshot(&story_id, limits()).await.expect("load");
    assert_eq!(
        snapshot.story_continuity().summary().summarized_through,
        Some(StorySequence::try_new(1).unwrap())
    );
    let _ = std::fs::remove_file(&db);
}
