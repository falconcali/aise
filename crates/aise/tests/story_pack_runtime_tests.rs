use aise::config::AssetLimitsConfig;
use aise::domain::asset::ids::{PackId, PlayerId, StoryRoleKey};
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
        .join(format!("aise_pack_runtime_{label}_{now}.db"))
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

async fn runtime_services(label: &str) -> (Arc<PackService>, Arc<StoryInstanceFactory>) {
    let (pack_service, instance_factory, _) = runtime_services_with_url(label).await;
    (pack_service, instance_factory)
}

async fn runtime_services_with_url(label: &str) -> (Arc<PackService>, Arc<StoryInstanceFactory>, String) {
    let db_url = temp_db_path(label);
    let store: Arc<dyn Store> = SqliteStore::connect(&db_url).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db_url).await.unwrap();
    let importer = NativeAssetImporter::new(AssetLimitsConfig::default());
    let pack_service = Arc::new(PackService::new(importer, asset_store.clone()));
    let instance_factory = Arc::new(StoryInstanceFactory::new(
        asset_store,
        store,
        StoryInstantiationLimits {
            max_roles: 32,
            max_facts: 512,
            max_rumors: 256,
            max_memories: 32,
            max_relationships: 32,
        },
    ));
    (pack_service, instance_factory, db_url)
}

#[tokio::test]
async fn pack_import_then_instance_create_roundtrip() {
    let (pack_service, instance_factory) = runtime_services("roundtrip").await;
    let json = valid_pack_json();
    let input = AssetInput::Json(json.as_bytes());
    let info = pack_service.import(input).await.expect("pack import should succeed");
    assert_eq!(info.pack_key.as_str(), "demo");

    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::from("player-1"),
        player_role_key: StoryRoleKey::from("protagonist"),
        player_character: None,
        created_at_ms: now_millis(),
    };
    let story_info = instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    assert_eq!(story_info.base_revision.get(), 0);
    assert!(!story_info.story_id.to_string().is_empty());
}

#[tokio::test]
async fn import_is_idempotent_by_digest() {
    let (pack_service, _) = runtime_services("idempotent").await;
    let json = valid_pack_json();
    let input = AssetInput::Json(json.as_bytes());
    let first = pack_service.import(input).await.expect("first import should succeed");
    let input = AssetInput::Json(json.as_bytes());
    let second = pack_service.import(input).await.expect("second import should succeed");
    assert_eq!(first.pack_id.to_string(), second.pack_id.to_string());
    assert_eq!(first.digest.to_string(), second.digest.to_string());
}

#[tokio::test]
async fn reject_duplicate_key_version_with_different_digest() {
    let (pack_service, _) = runtime_services("conflict").await;
    let json = valid_pack_json();
    pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("first import should succeed");
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["story"]["premise"] = serde_json::json!("changed premise");
    let changed = value.to_string();
    let result = pack_service.import(AssetInput::Json(changed.as_bytes())).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn reject_instance_for_non_playable_role() {
    let (pack_service, instance_factory) = runtime_services("non_playable").await;
    let json = valid_pack_json();
    let info = pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::from("player-1"),
        player_role_key: StoryRoleKey::from("narrator"),
        player_character: None,
        created_at_ms: now_millis(),
    };
    let result = instance_factory.create(spec).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn reject_instance_for_missing_pack() {
    let (_, instance_factory) = runtime_services("missing_pack").await;
    let spec = CreateStoryInstanceSpec {
        pack_id: PackId::from("pack-missing"),
        player_id: PlayerId::from("player-1"),
        player_role_key: StoryRoleKey::from("protagonist"),
        player_character: None,
        created_at_ms: now_millis(),
    };
    let result = instance_factory.create(spec).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pack_list_and_delete_roundtrip() {
    let (pack_service, _) = runtime_services("list_delete").await;
    let json = valid_pack_json();
    let info = pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let summaries = pack_service.list().await.expect("pack list should succeed");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].pack_id, info.pack_id);
    assert_eq!(summaries[0].title, "Demo");
    let deleted = pack_service.delete(&info.pack_id).await.expect("pack delete should succeed");
    assert!(deleted);
    let summaries = pack_service.list().await.expect("pack list should succeed");
    assert!(summaries.is_empty());
}

#[tokio::test]
async fn pack_delete_missing_returns_false() {
    let (pack_service, _) = runtime_services("delete_missing").await;
    let deleted = pack_service
        .delete(&PackId::from("pack-missing"))
        .await
        .expect("delete missing pack should not error");
    assert!(!deleted);
}

#[tokio::test]
async fn instance_meta_exposes_binding_and_characters() {
    let (pack_service, instance_factory, db_url) = runtime_services_with_url("instance_meta").await;
    let json = valid_pack_json();
    let info = pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::from("player-1"),
        player_role_key: StoryRoleKey::from("protagonist"),
        player_character: None,
        created_at_ms: now_millis(),
    };
    let story_info = instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    let store: Arc<dyn Store> = SqliteStore::connect(&db_url).await.unwrap();
    let meta = store
        .load_story_instance_meta(&story_info.story_id)
        .await
        .expect("instance meta should load")
        .expect("instance meta should exist");
    assert_eq!(meta.pack_id, info.pack_id);
    assert_eq!(meta.bindings.len(), 1);
    let binding = meta.bindings.values().next().unwrap();
    assert_eq!(binding.role_key.as_str(), "protagonist");
    assert!(binding.player_id.is_some());
    assert_eq!(meta.characters.len(), 1);
    let character = meta.characters.values().next().unwrap();
    assert_eq!(character.role_key.as_str(), "protagonist");
}

#[tokio::test]
async fn instance_snapshot_loads_with_scene_and_binding() {
    let (pack_service, instance_factory, db_url) = runtime_services_with_url("instance_snapshot").await;
    let json = valid_pack_json();
    let info = pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::from("player-1"),
        player_role_key: StoryRoleKey::from("protagonist"),
        player_character: None,
        created_at_ms: now_millis(),
    };
    let story_info = instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    let store: Arc<dyn Store> = SqliteStore::connect(&db_url).await.unwrap();
    let limits = aise::core::turn_data::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
    );
    let snapshot = store
        .load_story_snapshot(&story_info.story_id, limits)
        .await
        .expect("instance snapshot should load");
    assert_eq!(snapshot.current_scene().description.as_str(), "The village wakes.");
    assert!(snapshot.story_continuity().recent_segments().is_empty());
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
