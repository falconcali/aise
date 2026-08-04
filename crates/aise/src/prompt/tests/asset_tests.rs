use super::*;

fn make_manifest() -> PromptAssetManifest {
    PromptAssetManifest {
        asset_id: "intent/analysis".into(),
        kind: PromptKind::Text,
        source_path: "files/intent.md.j2".to_string(),
        input_schema_ref: None,
        output_contract_ref: None,
        labels: HashMap::new(),
        hash: None,
        status: AssetStatus::Active,
    }
}

#[test]
fn hash_is_stable() {
    let manifest = make_manifest();
    let content = "You are a helpful assistant.\n{{ context }}";

    let first = compute_asset_hash(content, &manifest);
    let second = compute_asset_hash(content, &manifest);

    assert_eq!(first, second);
    assert!(first.starts_with("sha256:"));
}

#[test]
fn hash_changes_with_content() {
    let manifest = make_manifest();

    let first = compute_asset_hash("template A", &manifest);
    let second = compute_asset_hash("template B", &manifest);

    assert_ne!(first, second);
}

#[test]
fn hash_normalizes_trailing_whitespace() {
    let manifest = make_manifest();

    let first = compute_asset_hash("line one  \nline two  ", &manifest);
    let second = compute_asset_hash("line one\nline two", &manifest);

    assert_eq!(first, second);
}

#[test]
fn hash_changes_when_schema_or_contract_changes() {
    let mut with_schema = make_manifest();
    with_schema.input_schema_ref = Some("schema://input".to_string());

    let mut with_contract = make_manifest();
    with_contract.output_contract_ref = Some("contract://output".to_string());

    let baseline = compute_asset_hash("hello", &make_manifest());
    let schema_hash = compute_asset_hash("hello", &with_schema);
    let contract_hash = compute_asset_hash("hello", &with_contract);

    assert_ne!(baseline, schema_hash);
    assert_ne!(baseline, contract_hash);
}

#[test]
fn yaml_deserialization_applies_defaults() {
    let yaml = r#"
asset_id: intent/analysis
kind: text
source_path: files/intent.md.j2
"#;

    let manifest: PromptAssetManifest = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(manifest.asset_id, "intent/analysis");
    assert_eq!(manifest.kind, PromptKind::Text);
    assert_eq!(manifest.status, AssetStatus::Active);
    assert!(manifest.labels.is_empty());
    assert!(manifest.hash.is_none());
}

#[test]
fn yaml_rejects_unknown_field() {
    let yaml = r#"
asset_id: intent/analysis
kind: text
source_path: files/intent.md.j2
revision: 1
"#;

    let err = serde_yaml::from_str::<PromptAssetManifest>(yaml).unwrap_err().to_string();

    assert!(err.contains("unknown field"));
    assert!(err.contains("revision"));
}
