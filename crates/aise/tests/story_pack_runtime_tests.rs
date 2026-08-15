use aise::config::{AssetLimitsConfig, NarrativeConfig};
use aise::domain::asset::frozen_ref::FrozenCharacterCardRef;
use aise::domain::asset::ids::{PackId, PlayerId, SemanticVersion};
use aise::domain::ids::RoleId;
use aise::domain::narrative::StorySegmentOrigin;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{
    SqliteStore, SqliteStoryHistoryReader, Store, StoryHistoryConfig, StoryHistoryQuery, StoryHistoryReadPort,
};
use aise::story::character_card_service::CharacterCardService;
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::collections::BTreeMap;
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
                    "appearance": "A mud-stained travel coat.",
                    "personality": "Cautious and curious.",
                    "speaking_style": "Concise and probing.",
                    "dialogue_examples": []
                },
                "background": "Grew up in the village.",
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": []
            },
            "narrator": {
                "role_label": "Narrator",
                "narrative_function": "observer",
                "default_profile": {
                    "name": "The Narrator",
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

fn valid_card_json(character_id: &str) -> String {
    serde_json::json!({
        "spec": "aise_char_v4",
        "spec_version": "4.0",
        "character_id": character_id,
        "meta": {
            "creator": "aise-team",
            "version": "1.0.0",
            "tags": []
        },
        "profile": {
            "name": "Selected Traveler",
            "appearance": "A patched cloak.",
            "personality": "Wary but kind.",
            "speaking_style": "Short sentences.",
            "dialogue_examples": []
        }
    })
    .to_string()
}

struct RuntimeServices {
    pack_service: Arc<PackService>,
    instance_factory: Arc<StoryInstanceFactory>,
    character_card_service: Arc<CharacterCardService>,
    db_url: String,
}

async fn runtime_services(label: &str) -> RuntimeServices {
    let db_url = temp_db_path(label);
    let sqlite = SqliteStore::connect(&db_url).await.unwrap();
    let store: Arc<dyn Store> = sqlite.clone();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db_url).await.unwrap();
    let importer = NativeAssetImporter::new(AssetLimitsConfig::default(), NarrativeConfig::default());
    let pack_service = Arc::new(PackService::new(importer, asset_store.clone()));
    let character_card_service = Arc::new(CharacterCardService::new(asset_store.clone(), AssetLimitsConfig::default()));
    let instance_factory = Arc::new(StoryInstanceFactory::new(
        asset_store,
        store,
        StoryInstantiationLimits {
            max_roles: 32,
            max_role_bytes: 131_072,
            max_facts: 512,
            max_rumors: 256,
            max_memories: 32,
            max_relationships: 32,
            max_opening_bytes: 8192,
        },
        NarrativeConfig::default().as_limits(),
    ));
    RuntimeServices {
        pack_service,
        instance_factory,
        character_card_service,
        db_url,
    }
}

#[tokio::test]
async fn pack_import_then_instance_create_roundtrip() {
    let services = runtime_services("roundtrip").await;
    let json = valid_pack_json();
    let input = AssetInput::Json(json.as_bytes());
    let info = services.pack_service.import(input).await.expect("pack import should succeed");
    assert_eq!(info.pack_key.as_str(), "demo");

    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("protagonist").unwrap(),
        role_profile_selections: BTreeMap::new(),
        created_at_ms: now_millis(),
    };
    let story_info = services
        .instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    assert_eq!(story_info.base_revision.get(), 0);
    assert!(!story_info.story_id.to_string().is_empty());
}

#[tokio::test]
async fn instance_create_with_selected_character_card_uses_card_profile() {
    let services = runtime_services("selected_card").await;
    let json = valid_pack_json();
    let info = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let character_id = uuid::Uuid::new_v4().to_string();
    let card_info = services
        .character_card_service
        .import(valid_card_json(&character_id).as_bytes())
        .await
        .expect("character card import should succeed");
    let reference = FrozenCharacterCardRef {
        character_id: card_info.character_id.clone(),
        version: SemanticVersion::try_new("1.0.0").unwrap(),
        digest: card_info.digest.clone(),
    };
    let mut role_profile_selections = BTreeMap::new();
    role_profile_selections.insert(RoleId::try_new("protagonist").unwrap(), reference);
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("protagonist").unwrap(),
        role_profile_selections,
        created_at_ms: now_millis(),
    };
    let story_info = services.instance_factory.create(spec).await.expect("creation should succeed");
    let store: Arc<dyn Store> = SqliteStore::connect(&services.db_url).await.unwrap();
    let meta = store
        .load_story_instance_meta(&story_info.story_id)
        .await
        .expect("meta should load")
        .expect("meta should exist");
    let protagonist = meta.roles.get(&RoleId::try_new("protagonist").unwrap()).unwrap();
    assert_eq!(protagonist.effective_profile.name.as_str(), "Selected Traveler");
    assert_eq!(
        protagonist.source_character.as_ref().unwrap().character_id,
        card_info.character_id
    );
    let narrator = meta.roles.get(&RoleId::try_new("narrator").unwrap()).unwrap();
    assert_eq!(narrator.effective_profile.name.as_str(), "The Narrator");
    assert!(narrator.source_character.is_none());
}

#[tokio::test]
async fn import_is_idempotent_by_digest() {
    let services = runtime_services("idempotent").await;
    let json = valid_pack_json();
    let first = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("first import should succeed");
    let second = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("second import should succeed");
    assert_eq!(first.pack_id.to_string(), second.pack_id.to_string());
    assert_eq!(first.digest.to_string(), second.digest.to_string());
}

#[tokio::test]
async fn reject_duplicate_key_version_with_different_digest() {
    let services = runtime_services("conflict").await;
    let json = valid_pack_json();
    services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("first import should succeed");
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["story"]["premise"] = serde_json::json!("changed premise");
    let changed = value.to_string();
    let result = services.pack_service.import(AssetInput::Json(changed.as_bytes())).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pack_list_and_delete_roundtrip() {
    let services = runtime_services("list_delete").await;
    let json = valid_pack_json();
    let info = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let summaries = services.pack_service.list().await.expect("pack list should succeed");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].pack_id, info.pack_id);
    assert_eq!(summaries[0].title, "Demo");
    let deleted = services
        .pack_service
        .delete(&info.pack_id)
        .await
        .expect("pack delete should succeed");
    assert!(deleted);
    let summaries = services.pack_service.list().await.expect("pack list should succeed");
    assert!(summaries.is_empty());
}

#[tokio::test]
async fn pack_delete_missing_returns_false() {
    let services = runtime_services("delete_missing").await;
    let deleted = services
        .pack_service
        .delete(&PackId::from("pack-missing"))
        .await
        .expect("delete missing pack should not error");
    assert!(!deleted);
}

#[tokio::test]
async fn reject_instance_for_non_playable_role() {
    let services = runtime_services("non_playable").await;
    let json = valid_pack_json();
    let info = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("narrator").unwrap(),
        role_profile_selections: BTreeMap::new(),
        created_at_ms: now_millis(),
    };
    let result = services.instance_factory.create(spec).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn reject_instance_for_missing_pack() {
    let services = runtime_services("missing_pack").await;
    let spec = CreateStoryInstanceSpec {
        pack_id: PackId::from("pack-missing"),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("protagonist").unwrap(),
        role_profile_selections: BTreeMap::new(),
        created_at_ms: now_millis(),
    };
    let result = services.instance_factory.create(spec).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn instance_meta_exposes_roles() {
    let services = runtime_services("instance_meta").await;
    let json = valid_pack_json();
    let info = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("protagonist").unwrap(),
        role_profile_selections: BTreeMap::new(),
        created_at_ms: now_millis(),
    };
    let story_info = services
        .instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    let store: Arc<dyn Store> = SqliteStore::connect(&services.db_url).await.unwrap();
    let meta = store
        .load_story_instance_meta(&story_info.story_id)
        .await
        .expect("instance meta should load")
        .expect("instance meta should exist");
    assert_eq!(meta.pack_id, info.pack_id);
    assert_eq!(meta.roles.len(), 2);
    let protagonist = meta.roles.get(&RoleId::try_new("protagonist").unwrap()).unwrap();
    assert!(protagonist.is_player_controlled());
    let narrator = meta.roles.get(&RoleId::try_new("narrator").unwrap()).unwrap();
    assert!(!narrator.is_player_controlled());
}

#[tokio::test]
async fn instance_snapshot_loads_with_scene_and_roles() {
    let services = runtime_services("instance_snapshot").await;
    let json = valid_pack_json();
    let info = services
        .pack_service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack import should succeed");
    let spec = CreateStoryInstanceSpec {
        pack_id: info.pack_id.clone(),
        player_id: PlayerId::try_new("player-1").unwrap(),
        player_role_id: RoleId::try_new("protagonist").unwrap(),
        role_profile_selections: BTreeMap::new(),
        created_at_ms: now_millis(),
    };
    let story_info = services
        .instance_factory
        .create(spec)
        .await
        .expect("story instance creation should succeed");
    let store: Arc<dyn Store> = SqliteStore::connect(&services.db_url).await.unwrap();
    let limits = aise::domain::turn::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
        &NarrativeConfig::default(),
    );
    let snapshot = store
        .load_story_snapshot(&story_info.story_id, limits)
        .await
        .expect("instance snapshot should load");
    assert_eq!(snapshot.current_scene().description.as_str(), "The village wakes.");
    assert_eq!(snapshot.player_role_id(), &RoleId::try_new("protagonist").unwrap());
    assert!(snapshot.player_role().is_player_controlled());
    assert_eq!(snapshot.roles().len(), 2);
    assert!(snapshot.story_continuity().summary().text.as_str().is_empty());
    assert_eq!(snapshot.story_continuity().recent_segments().len(), 1);
    let opening = &snapshot.story_continuity().recent_segments()[0];
    assert_eq!(opening.sequence.get(), 1);
    assert_eq!(opening.origin, StorySegmentOrigin::Opening);
    assert_eq!(opening.text.as_str(), "You open your eyes.");

    let sqlite = SqliteStore::connect(&services.db_url).await.unwrap();
    let history = SqliteStoryHistoryReader::new(sqlite, StoryHistoryConfig::default())
        .unwrap()
        .load_story_history(
            &story_info.story_id,
            StoryHistoryQuery {
                after_sequence: None,
                limit: 20,
            },
        )
        .await
        .unwrap();
    assert_eq!(history.opening.unwrap().story_text, "You open your eyes.");
    assert!(history.turns.is_empty());
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
