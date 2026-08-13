use aise::config::AssetLimitsConfig;
use aise::domain::asset::ids::{PlayerId, StoryRoleKey};
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_engine_flow_{label}_{now}.db"))
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
            "opening": "You open your eyes."
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

#[tokio::test]
async fn create_story_instance_flow_materializes_snapshot() {
    let db = temp_db_path("instance");
    let store: Arc<dyn Store> = SqliteStore::connect(&db).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(NativeAssetImporter::new(AssetLimitsConfig::default()), asset_store.clone());
    let pack = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("import");
    let factory = StoryInstanceFactory::new(
        asset_store,
        store.clone(),
        StoryInstantiationLimits {
            max_roles: 16,
            max_facts: 128,
            max_rumors: 128,
            max_memories: 128,
            max_relationships: 64,
            max_opening_bytes: 8192,
        },
    );
    let story = factory
        .create(CreateStoryInstanceSpec {
            pack_id: pack.pack_id,
            player_id: PlayerId::from("player-1"),
            player_role_key: StoryRoleKey::from("protagonist"),
            player_character: None,
            created_at_ms: 1,
        })
        .await
        .expect("create");
    let limits = aise::domain::turn::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
    );
    let snapshot = store.load_story_snapshot(&story.story_id, limits).await.expect("snapshot");
    assert_eq!(snapshot.current_scene().description.as_str(), "The village wakes.");
    assert_eq!(snapshot.story_continuity().recent_segments().len(), 1);
    assert_eq!(
        snapshot.story_continuity().recent_segments()[0].text.as_str(),
        "You open your eyes."
    );
    let _ = std::fs::remove_file(&db);
}
