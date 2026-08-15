use aise::config::{AssetLimitsConfig, NarrativeConfig};
use aise::domain::asset::validation::AssetValidationCode;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::sqlite_store::SqliteStore;
use aise::story::pack_service::{AssetImportError, AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn importer() -> NativeAssetImporter {
    NativeAssetImporter::new(AssetLimitsConfig::default(), NarrativeConfig::default())
}

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_asset_import_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

async fn pack_service(label: &str) -> (PackService, String) {
    let db = temp_db_path(label);
    let _ = SqliteStore::connect(&db).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let service = PackService::new(importer(), asset_store);
    (service, db)
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

#[test]
fn valid_pack_passes_validation() {
    let importer = importer();
    let report = importer.parse(AssetInput::Json(valid_pack_json().as_bytes()));
    assert!(report.valid, "expected valid pack, got issues: {:?}", report.issues);
}

#[test]
fn rejects_unknown_spec() {
    let importer = importer();
    let json = valid_pack_json().replace("\"aise_story_v4\"", "\"aise_story_v2\"");
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::UnsupportedSpec)
    );
}

#[test]
fn rejects_unsupported_spec_version() {
    let importer = importer();
    let json = valid_pack_json().replace("\"4.0\"", "\"5.0\"");
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::UnsupportedSpecVersion)
    );
}

#[test]
fn rejects_crossed_spec_and_version_pair() {
    let importer = importer();
    let json = valid_pack_json().replace("\"4.0\"", "\"3.0\"");
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::UnsupportedSpecVersion)
    );
}

#[test]
fn rejects_forbidden_runtime_fields() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["story"]["system_prompt"] = serde_json::json!("injected");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::ForbiddenField)
    );
}

#[test]
fn rejects_missing_story_opening() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["start"]["opening"] = serde_json::json!("");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingStoryOpening)
    );
}

#[test]
fn rejects_legacy_role_openings() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["start"]["role_openings"] = serde_json::json!({
        "protagonist": "You open your eyes."
    });
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| { issue.code == AssetValidationCode::SchemaInvalid && issue.path == "/start/role_openings" })
    );
}

#[test]
fn rejects_missing_default_profile() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"].as_object_mut().unwrap().remove("default_profile");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report.issues.iter().any(|issue| {
            issue.code == AssetValidationCode::SchemaInvalid && issue.path.ends_with("default_profile")
        })
    );
}

#[test]
fn rejects_empty_profile_name() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["default_profile"]["name"] = serde_json::json!("");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(report.issues.iter().any(|issue| issue.code == AssetValidationCode::EmptyText));
}

#[test]
fn rejects_empty_optional_profile_field() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["default_profile"]["appearance"] = serde_json::json!("");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(report.issues.iter().any(|issue| issue.code == AssetValidationCode::EmptyText));
}

#[test]
fn accepts_profile_with_absent_optional_fields() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["default_profile"] = serde_json::json!({
        "name": "Minimal"
    });
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(report.valid, "expected valid pack, got issues: {:?}", report.issues);
}

#[test]
fn rejects_profile_name_over_limit() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    let limits = AssetLimitsConfig::default();
    let oversized = "a".repeat(limits.max_profile_name_bytes + 1);
    value["roles"]["protagonist"]["default_profile"]["name"] = serde_json::json!(oversized);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::LimitExceeded)
    );
}

#[test]
fn accepts_profile_name_at_exact_limit() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    let limits = AssetLimitsConfig::default();
    let exact = "a".repeat(limits.max_profile_name_bytes);
    value["roles"]["protagonist"]["default_profile"]["name"] = serde_json::json!(exact);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(report.valid, "expected valid pack, got issues: {:?}", report.issues);
}

#[test]
fn rejects_dialogue_example_count_overflow() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    let limits = AssetLimitsConfig::default();
    let examples: Vec<serde_json::Value> = (0..limits.max_dialogue_examples_per_profile + 1)
        .map(|index| serde_json::json!({"situation": format!("s{index}"), "response": format!("r{index}")}))
        .collect();
    value["roles"]["protagonist"]["default_profile"]["dialogue_examples"] = serde_json::json!(examples);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::LimitExceeded)
    );
}

#[test]
fn rejects_unknown_profile_field() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["default_profile"]["description"] = serde_json::json!("legacy field");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report.issues.iter().any(|issue| {
            issue.code == AssetValidationCode::SchemaInvalid && issue.path.ends_with("default_profile")
        })
    );
}

#[test]
fn rejects_role_background_over_limit() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    let limits = AssetLimitsConfig::default();
    let oversized = "a".repeat(limits.max_role_background_bytes + 1);
    value["roles"]["protagonist"]["background"] = serde_json::json!(oversized);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::LimitExceeded)
    );
}

#[test]
fn rejects_empty_role_background() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["background"] = serde_json::json!("");
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(report.issues.iter().any(|issue| issue.code == AssetValidationCode::EmptyText));
}

#[test]
fn accepts_pack_with_zero_character_cards_and_no_default_cast() {
    let importer = importer();
    let value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    assert!(value.get("character_assets").is_none());
    assert!(value.get("default_cast").is_none());
    let report = importer.parse(AssetInput::Json(value.to_string().as_bytes()));
    assert!(report.valid, "expected valid pack, got issues: {:?}", report.issues);
}

#[test]
fn rejects_unknown_relationship_target_role() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["roles"]["protagonist"]["initial_relationships"] = serde_json::json!([
        {"target_role_id": "unknown_role", "kind": "ally", "trust": 10}
    ]);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingReference)
    );
}

#[test]
fn rejects_unknown_playable_role() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["play"]["playable_role_ids"] = serde_json::json!(["unknown_role"]);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingReference)
    );
}

#[test]
fn rejects_unknown_narrative_role_reference() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["narrative"]["nodes"]["node_a"]["activate_when"] = serde_json::json!({
        "type": "role_controller_is",
        "role_key": "unknown_role",
        "controller": "player"
    });
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingReference)
    );
}

#[test]
fn rejects_salience_out_of_range() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["world_book"]["facts"]["fact_a"] = serde_json::json!({
        "content": "a fact",
        "salience": 101
    });
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::InvalidSalience)
    );
}

#[test]
fn rejects_graph_cycle() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["narrative"]["entry_nodes"] = serde_json::json!(["node_a"]);
    value["narrative"]["nodes"]["node_b"] = serde_json::json!({
        "title": "B",
        "dramatic_focus": "B",
        "activate_when": {"type": "node_state", "node_key": "node_a", "state": "completed"},
        "complete_when": {"type": "turn_reaches", "turn": 99},
        "skip_when": null,
        "effects": {"on_activate": [], "on_complete": []},
        "terminal": false
    });
    value["narrative"]["edges"] = serde_json::json!([
        {"edge_key": "e1", "from": "node_a", "to": "node_b", "when": {"type": "story_started"}},
        {"edge_key": "e2", "from": "node_b", "to": "node_a", "when": {"type": "story_started"}}
    ]);
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(report.issues.iter().any(|issue| issue.code == AssetValidationCode::GraphCycle));
}

#[test]
fn rejects_unreachable_graph_node() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["narrative"]["nodes"]["orphan"] = serde_json::json!({
        "title": "O",
        "dramatic_focus": "O",
        "activate_when": {"type": "turn_reaches", "turn": 5},
        "complete_when": {"type": "turn_reaches", "turn": 6},
        "skip_when": null,
        "effects": {"on_activate": [], "on_complete": []},
        "terminal": false
    });
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::GraphUnreachable)
    );
}

#[test]
fn rejects_pack_container_without_manifest() {
    let importer = importer();
    let report = importer.parse(AssetInput::Pack(&[]));
    assert!(!report.valid);
}

#[test]
fn rejects_traversal_archive_paths() {
    let importer = importer();
    let json = valid_pack_json();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("story.aise.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut writer, json.as_bytes()).unwrap();
    writer
        .start_file("../evil.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut writer, b"evil").unwrap();
    let cursor = writer.finish().unwrap();
    let bytes = cursor.into_inner();
    let report = importer.parse(AssetInput::Pack(&bytes));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::ArchivePathUnsafe)
    );
}

#[test]
fn accepts_pack_container_with_manifest() {
    let importer = importer();
    let json = valid_pack_json();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("story.aise.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut writer, json.as_bytes()).unwrap();
    let cursor = writer.finish().unwrap();
    let bytes = cursor.into_inner();
    let report = importer.parse(AssetInput::Pack(&bytes));
    assert!(report.valid, "expected valid pack container, got: {:?}", report.issues);
}

#[test]
fn rejects_missing_manifest_in_pack_container() {
    let importer = importer();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("assets/pic.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut writer, b"png").unwrap();
    let cursor = writer.finish().unwrap();
    let bytes = cursor.into_inner();
    let report = importer.parse(AssetInput::Pack(&bytes));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingReference)
    );
}

#[tokio::test]
async fn full_import_rejects_v3_character_assets_field() {
    let (service, db) = pack_service("v3_character_assets").await;
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["character_assets"] = serde_json::json!({});
    let error = service
        .import(AssetInput::Json(value.to_string().as_bytes()))
        .await
        .expect_err("v3 character_assets field must be rejected");
    assert!(matches!(error, AssetImportError::Invalid(_)));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn full_import_rejects_v3_default_cast_field() {
    let (service, db) = pack_service("v3_default_cast").await;
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["default_cast"] = serde_json::json!({});
    let error = service
        .import(AssetInput::Json(value.to_string().as_bytes()))
        .await
        .expect_err("v3 default_cast field must be rejected");
    assert!(matches!(error, AssetImportError::Invalid(_)));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn full_import_of_zero_character_card_pack_succeeds_and_round_trips() {
    let (service, db) = pack_service("zero_cards_round_trip").await;
    let json = valid_pack_json();
    let info = service
        .import(AssetInput::Json(json.as_bytes()))
        .await
        .expect("pack with zero character cards should import");
    assert_eq!(info.pack_key.as_str(), "demo");
    let exported = service
        .export(&info.pack_id, aise::story::pack_service::PackExportFormat::Json)
        .await
        .expect("export should succeed");
    match exported {
        aise::story::pack_service::PackExport::Json(bytes) => {
            let mut original: serde_json::Value = serde_json::from_str(&json).unwrap();
            original["roles"]["protagonist"]["initial_state"]["attributes"] = serde_json::json!({});
            original["world_book"]["topics"] = serde_json::json!({});
            let round_tripped: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(original, round_tripped);
        }
        other => panic!("expected JSON export, got {other:?}"),
    }
    let _ = std::fs::remove_file(&db);
}
