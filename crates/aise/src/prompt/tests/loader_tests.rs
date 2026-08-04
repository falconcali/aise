use super::*;
use crate::prompt::{
    model::{AssetStatus, PromptKind},
    resolver::PromptRenderOptions,
    slot::{SlotSpec, VarSpec, VarType},
};
use serde_json::json;
use std::{collections::HashMap, path::Path};
use tempfile::TempDir;

fn make_manifest(asset_id: &str, kind: PromptKind, source_path: &str) -> PromptAssetManifest {
    PromptAssetManifest {
        asset_id: asset_id.into(),
        kind,
        source_path: source_path.to_string(),
        input_schema_ref: None,
        output_contract_ref: None,
        labels: HashMap::new(),
        hash: None,
        status: AssetStatus::Active,
    }
}

fn manifest_hash(asset_id: &str, kind: PromptKind, source_path: &str, body: &str) -> String {
    compute_asset_hash(body, &make_manifest(asset_id, kind, source_path))
}

fn section(asset_id: &str, slot_ids: &[&str], body: &str) -> String {
    let metadata = serde_json::json!({
        "asset_id": asset_id,
        "slot_ids": slot_ids,
    });
    format!("{{# @asset {} #}}\n{}\n{{# @endasset #}}\n", metadata, body)
}

fn write_file(dir: &Path, relative_path: &str, content: &str) {
    let path = dir.join(relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn write_minimal_prompt_dir(dir: &Path) {
    let source_path = "files/intent.md.j2";
    let body = "Analyze: {{ user_input }}";
    write_file(dir, source_path, &section("intent/analysis", &["intent.analysis"], body));

    let hash = manifest_hash("intent/analysis", PromptKind::Text, source_path, body);
    let index = format!(
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    hash: "{hash}"
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#
    );
    write_file(dir, "index.yaml", &index);

    let slots = r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    required: true
    vars:
      - { name: user_input, var_type: string, required: true }
"#;
    write_file(dir, "slots.yaml", slots);
}

fn make_slot_spec(slot_id: &str, allowed_kinds: Vec<PromptKind>, vars: Vec<VarSpec>) -> SlotSpec {
    SlotSpec {
        slot_id: slot_id.into(),
        allowed_kinds,
        required: false,
        structured_output: false,
        output_contract_required: false,
        optimizable: false,
        allow_child_render: false,
        notes: None,
        vars,
        output_contract: None,
    }
}

#[test]
fn manifests_by_source_path_groups_entries() {
    let manifests = vec![
        make_manifest("intent/analysis", PromptKind::Text, "files/shared.md.j2"),
        make_manifest("summary/main", PromptKind::Text, "files/shared.md.j2"),
        make_manifest("plan/system", PromptKind::Text, "files/plan.md.j2"),
    ];

    let grouped = manifests_by_source_path(manifests);

    assert_eq!(grouped["files/shared.md.j2"].len(), 2);
    assert_eq!(grouped["files/plan.md.j2"].len(), 1);
}

#[test]
fn validate_section_slot_compatibility_rejects_unknown_slot() {
    let section = PromptAssetSection {
        asset_id: "intent/analysis".into(),
        slot_ids: vec!["unknown.slot".into()],
        body: "Analyze".to_string(),
        source_anchor: "files/intent.md.j2#intent/analysis".to_string(),
    };

    let err = validate_section_slot_compatibility(&section, &SlotRegistry::new())
        .unwrap_err()
        .to_string();

    assert!(err.contains("unknown slot `unknown.slot`"));
}

#[test]
fn validate_section_slot_compatibility_rejects_different_allowed_kinds() {
    let section = PromptAssetSection {
        asset_id: "shared/asset".into(),
        slot_ids: vec!["slot.a".into(), "slot.b".into()],
        body: "shared".to_string(),
        source_anchor: "files/shared.md.j2#shared/asset".to_string(),
    };
    let slots = HashMap::from([
        ("slot.a".into(), make_slot_spec("slot.a", vec![PromptKind::Text], Vec::new())),
        (
            "slot.b".into(),
            make_slot_spec("slot.b", vec![PromptKind::Messages], Vec::new()),
        ),
    ]);

    let err = validate_section_slot_compatibility(&section, &slots).unwrap_err().to_string();

    assert!(err.contains("different allowed_kinds"));
}

#[test]
fn validate_section_slot_compatibility_rejects_different_var_requirements() {
    let section = PromptAssetSection {
        asset_id: "shared/asset".into(),
        slot_ids: vec!["slot.a".into(), "slot.b".into()],
        body: "shared".to_string(),
        source_anchor: "files/shared.md.j2#shared/asset".to_string(),
    };
    let slots = HashMap::from([
        (
            "slot.a".into(),
            make_slot_spec(
                "slot.a",
                vec![PromptKind::Text],
                vec![VarSpec {
                    name: "title".to_string(),
                    var_type: VarType::String,
                    required: true,
                }],
            ),
        ),
        (
            "slot.b".into(),
            make_slot_spec(
                "slot.b",
                vec![PromptKind::Text],
                vec![VarSpec {
                    name: "count".to_string(),
                    var_type: VarType::Number,
                    required: true,
                }],
            ),
        ),
    ]);

    let err = validate_section_slot_compatibility(&section, &slots).unwrap_err().to_string();

    assert!(err.contains("different variable requirements"));
}

#[test]
fn load_catalog_end_to_end() {
    let tmp = TempDir::new().unwrap();
    write_minimal_prompt_dir(tmp.path());

    let catalog = load_catalog(tmp.path()).unwrap();
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);
    let text = catalog
        .render_text("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap();

    assert_eq!(catalog.asset_count(), 1);
    assert_eq!(catalog.pack_count(), 1);
    assert_eq!(text, "Analyze: hello");
}

#[test]
fn load_catalog_fails_with_missing_index() {
    let tmp = TempDir::new().unwrap();

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("failed to read"));
    assert!(err.contains("index.yaml"));
}

#[test]
fn load_catalog_fails_with_hash_mismatch() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/intent.md.j2",
        &section("intent/analysis", &["intent.analysis"], "template content"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    hash: "sha256:wrong"
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("hash mismatch"));
    assert!(err.contains("intent/analysis"));
}

#[test]
fn load_catalog_fails_with_duplicate_pack_names() {
    let tmp = TempDir::new().unwrap();
    write_minimal_prompt_dir(tmp.path());

    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("duplicate pack name `default`"));
}

#[test]
fn load_catalog_rejects_unknown_index_fields() {
    let tmp = TempDir::new().unwrap();
    write_minimal_prompt_dir(tmp.path());

    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
unexpected: true
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("unknown field"));
    assert!(err.contains("unexpected"));
}

#[test]
fn load_catalog_rejects_unknown_pack_fields() {
    let tmp = TempDir::new().unwrap();
    write_minimal_prompt_dir(tmp.path());

    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    priority: 1
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("unknown field"));
    assert!(err.contains("priority"));
}

#[test]
fn load_catalog_rejects_non_active_asset_status() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/intent.md.j2",
        &section("intent/analysis", &["intent.analysis"], "Analyze"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: deprecated
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("unsupported status `deprecated`"));
}

#[test]
fn load_catalog_rejects_unsupported_policy_types() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/intent.md.j2",
        &section("intent/analysis", &["intent.analysis"], "Analyze"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
policies:
  - type: runtime_guard
    name: safe
    description: reject dangerous output
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("unsupported type `runtime_guard`"));
}

#[test]
fn load_catalog_rejects_unknown_policy_fields() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/intent.md.j2",
        &section("intent/analysis", &["intent.analysis"], "Analyze"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
policies:
  - type: preamble
    name: safe
    content: "[SAFE]"
    position: prepend
    unexpected: true
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("unknown field"));
    assert!(err.contains("unexpected"));
}

#[test]
fn load_catalog_computes_missing_hash_from_section_body() {
    let tmp = TempDir::new().unwrap();
    let source_path = "files/shared.md.j2";
    let first_body = "first body";
    let second_body = "second body";
    write_file(
        tmp.path(),
        source_path,
        &format!(
            "{}\n{}",
            section("intent/analysis", &["intent.analysis"], first_body),
            section("summary/main", &["summary.main"], second_body)
        ),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    status: active
  - asset_id: summary/main
    kind: text
    source_path: files/shared.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
      summary.main: "summary/main"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
  - slot_id: summary.main
    allowed_kinds: [text]
"#,
    );

    let catalog = load_catalog(tmp.path()).unwrap();

    assert_eq!(
        catalog.asset("intent/analysis").map(|asset| asset.resolved_hash.as_str()),
        Some(manifest_hash("intent/analysis", PromptKind::Text, source_path, first_body).as_str())
    );
    assert_eq!(
        catalog.asset("summary/main").map(|asset| asset.resolved_hash.as_str()),
        Some(manifest_hash("summary/main", PromptKind::Text, source_path, second_body).as_str())
    );
}

#[test]
fn load_catalog_supports_pack_inheritance() {
    let tmp = TempDir::new().unwrap();
    let source_path = "files/shared.md.j2";
    let intent_body = "base: {{ input }}";
    let summary_body = "summary: {{ input }}";
    write_file(
        tmp.path(),
        source_path,
        &format!(
            "{}\n{}",
            section("intent/analysis", &["intent.analysis"], intent_body),
            section("summary/main", &["summary.main"], summary_body)
        ),
    );

    let intent_hash = manifest_hash("intent/analysis", PromptKind::Text, source_path, intent_body);
    let summary_hash = manifest_hash("summary/main", PromptKind::Text, source_path, summary_body);
    let index = format!(
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    hash: "{intent_hash}"
    status: active
  - asset_id: summary/main
    kind: text
    source_path: files/shared.md.j2
    hash: "{summary_hash}"
    status: active
packs:
  - name: base
    slots:
      intent.analysis: "intent/analysis"
      summary.main: "summary/main"
  - name: child
    extends: base
    slots: {{}}
resolver:
  default_pack: child
"#
    );
    write_file(tmp.path(), "index.yaml", &index);
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    required: true
  - slot_id: summary.main
    allowed_kinds: [text]
    required: false
"#,
    );

    let catalog = load_catalog(tmp.path()).unwrap();
    let vars = HashMap::from([("input".to_string(), json!("test"))]);

    assert_eq!(
        catalog
            .render_text("intent.analysis", &vars, &PromptRenderOptions::default())
            .unwrap(),
        "base: test"
    );
    assert_eq!(
        catalog
            .render_text("summary.main", &vars, &PromptRenderOptions::default())
            .unwrap(),
        "summary: test"
    );
}

#[test]
fn load_catalog_fails_when_required_slot_is_uncovered() {
    let tmp = TempDir::new().unwrap();
    let source_path = "files/intent.md.j2";
    let body = "Analyze: {{ input }}";
    write_file(tmp.path(), source_path, &section("intent/analysis", &["intent.analysis"], body));

    let hash = manifest_hash("intent/analysis", PromptKind::Text, source_path, body);
    let index = format!(
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    hash: "{hash}"
    status: active
packs:
  - name: default
    slots: {{}}
resolver:
  default_pack: default
"#
    );
    write_file(tmp.path(), "index.yaml", &index);
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
    required: true
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("does not cover required slot `intent.analysis`"));
}

#[test]
fn load_catalog_fails_on_duplicate_asset_section() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/shared.md.j2",
        &format!(
            "{}\n{}",
            section("intent/analysis", &["intent.analysis"], "first"),
            section("intent/analysis", &["intent.analysis"], "second")
        ),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("duplicate asset section"));
}

#[test]
fn load_catalog_fails_when_manifest_section_is_missing() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/shared.md.j2",
        &section("summary/main", &["summary.main"], "summary body"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
  - slot_id: summary.main
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("does not have a matching section"));
}

#[test]
fn load_catalog_fails_when_file_contains_unregistered_asset() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/shared.md.j2",
        &format!(
            "{}\n{}",
            section("intent/analysis", &["intent.analysis"], "intent body"),
            section("summary/main", &["summary.main"], "summary body")
        ),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
  - slot_id: summary.main
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("is not registered in index.yaml"));
}

#[test]
fn load_catalog_fails_when_pack_slot_is_not_declared_in_section() {
    let tmp = TempDir::new().unwrap();
    let source_path = "files/shared.md.j2";
    let body = "Analyze: {{ input }}";
    write_file(tmp.path(), source_path, &section("intent/analysis", &["summary.main"], body));

    let hash = manifest_hash("intent/analysis", PromptKind::Text, source_path, body);
    let index = format!(
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/shared.md.j2
    hash: "{hash}"
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: default
"#
    );
    write_file(tmp.path(), "index.yaml", &index);
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
  - slot_id: summary.main
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("does not declare that slot"));
}

#[test]
fn load_catalog_fails_when_default_pack_is_missing() {
    let tmp = TempDir::new().unwrap();
    write_minimal_prompt_dir(tmp.path());

    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: actual
    slots:
      intent.analysis: "intent/analysis"
resolver:
  default_pack: missing
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("default_pack `missing` does not exist"));
}

#[test]
fn load_catalog_fails_when_pack_references_unknown_asset() {
    let tmp = TempDir::new().unwrap();

    write_file(
        tmp.path(),
        "files/intent.md.j2",
        &section("intent/analysis", &["intent.analysis"], "Analyze"),
    );
    write_file(
        tmp.path(),
        "index.yaml",
        r#"
assets:
  - asset_id: intent/analysis
    kind: text
    source_path: files/intent.md.j2
    status: active
packs:
  - name: default
    slots:
      intent.analysis: "intent/missing"
resolver:
  default_pack: default
"#,
    );
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: intent.analysis
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("references unknown asset `intent/missing`"));
}

#[test]
fn load_catalog_fails_when_pack_kind_mismatches_slot() {
    let tmp = TempDir::new().unwrap();
    let source_path = "files/messages.json.j2";
    let body = r#"[{"role":"system","content":"hello"}]"#;
    write_file(tmp.path(), source_path, &section("chat/messages", &["chat.messages"], body));

    let hash = manifest_hash("chat/messages", PromptKind::Messages, source_path, body);
    let index = format!(
        r#"
assets:
  - asset_id: chat/messages
    kind: messages
    source_path: files/messages.json.j2
    hash: "{hash}"
    status: active
packs:
  - name: default
    slots:
      chat.messages: "chat/messages"
resolver:
  default_pack: default
"#
    );
    write_file(tmp.path(), "index.yaml", &index);
    write_file(
        tmp.path(),
        "slots.yaml",
        r#"
slots:
  - slot_id: chat.messages
    allowed_kinds: [text]
"#,
    );

    let err = load_catalog(tmp.path()).unwrap_err().to_string();

    assert!(err.contains("kind mismatch"));
}
