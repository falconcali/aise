use aise::config::AssetLimitsConfig;
use aise::domain::asset::validation::AssetValidationCode;
use aise::story::pack_service::{AssetInput, NativeAssetImporter};

fn importer() -> NativeAssetImporter {
    NativeAssetImporter::new(AssetLimitsConfig::default())
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

#[test]
fn valid_pack_passes_validation() {
    let importer = importer();
    let report = importer.parse(AssetInput::Json(valid_pack_json().as_bytes()));
    assert!(report.valid, "expected valid pack, got issues: {:?}", report.issues);
}

#[test]
fn rejects_unknown_spec() {
    let importer = importer();
    let json = valid_pack_json().replace("\"aise_story_v3\"", "\"aise_story_v2\"");
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
fn rejects_unknown_spec_version() {
    let importer = importer();
    let json = valid_pack_json().replace("\"3.0\"", "\"4.0\"");
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
fn rejects_missing_default_cast() {
    let importer = importer();
    let mut value: serde_json::Value = serde_json::from_str(&valid_pack_json()).unwrap();
    value["default_cast"] = serde_json::json!({});
    let json = value.to_string();
    let report = importer.parse(AssetInput::Json(json.as_bytes()));
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == AssetValidationCode::MissingDefaultCast)
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
        "objective": "B",
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
        "objective": "O",
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
