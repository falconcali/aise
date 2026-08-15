use aise::config::{AssetLimitsConfig, NarrativeConfig};
use aise::domain::asset::ids::PlayerId;
use aise::domain::ids::RoleId;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_sse_{label}_{now}.db"))
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
                    "name": "The Traveler",
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

#[tokio::test]
async fn story_instance_snapshot_is_available_for_sse_recovery_path() {
    let db = temp_db_path("sse");
    let store: Arc<dyn Store> = SqliteStore::connect(&db).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(
        NativeAssetImporter::new(AssetLimitsConfig::default(), NarrativeConfig::default()),
        asset_store.clone(),
    );
    let pack = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("import");
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
            pack_id: pack.pack_id,
            player_id: PlayerId::try_new("player-1").unwrap(),
            player_role_id: RoleId::try_new("protagonist").unwrap(),
            role_profile_selections: BTreeMap::new(),
            created_at_ms: 1,
        })
        .await
        .expect("create");
    let limits = aise::domain::turn::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
        &NarrativeConfig::default(),
    );
    let snapshot = store.load_story_snapshot(&story.story_id, limits).await.expect("snapshot");
    assert_eq!(snapshot.story_id(), &story.story_id);
    let _ = std::fs::remove_file(&db);
}
