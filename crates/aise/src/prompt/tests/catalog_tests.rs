use super::*;
use crate::prompt::{
    asset::PromptAssetManifest,
    model::{AssetStatus, PromptMessage, PromptRole},
    policy::PreamblePosition,
    slot::{OutputContract, SlotSpec, VarSpec, VarType},
};
use serde_json::json;

fn make_slot(
    slot_id: &str,
    allowed_kinds: Vec<PromptKind>,
    vars: Vec<VarSpec>,
    output_contract: Option<OutputContract>,
) -> SlotSpec {
    SlotSpec {
        slot_id: slot_id.into(),
        allowed_kinds,
        required: true,
        structured_output: false,
        output_contract_required: false,
        optimizable: false,
        allow_child_render: false,
        notes: None,
        vars,
        output_contract,
    }
}

fn make_catalog() -> PromptCatalog {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("intent/analysis", "Analyze: {{ user_input }}").unwrap();
    renderer
        .add_template("chat/messages", r#"[{"role":"system","content":"You are {{ persona }}."}]"#)
        .unwrap();

    let assets = HashMap::from([
        (
            "intent/analysis".into(),
            CompiledPromptAsset {
                manifest: PromptAssetManifest {
                    asset_id: "intent/analysis".into(),
                    kind: PromptKind::Text,
                    source_path: "files/intent.md.j2".to_string(),
                    input_schema_ref: None,
                    output_contract_ref: None,
                    labels: HashMap::new(),
                    hash: None,
                    status: AssetStatus::Active,
                },
                source_anchor: "files/intent.md.j2#intent/analysis".to_string(),
                resolved_hash: "sha256:intent".to_string(),
                template_name: "intent/analysis".into(),
            },
        ),
        (
            "chat/messages".into(),
            CompiledPromptAsset {
                manifest: PromptAssetManifest {
                    asset_id: "chat/messages".into(),
                    kind: PromptKind::Messages,
                    source_path: "files/chat.json.j2".to_string(),
                    input_schema_ref: None,
                    output_contract_ref: None,
                    labels: HashMap::new(),
                    hash: None,
                    status: AssetStatus::Active,
                },
                source_anchor: "files/chat.json.j2#chat/messages".to_string(),
                resolved_hash: "sha256:messages".to_string(),
                template_name: "chat/messages".into(),
            },
        ),
    ]);

    let slots = HashMap::from([
        (
            "intent.analysis".into(),
            make_slot(
                "intent.analysis",
                vec![PromptKind::Text],
                vec![VarSpec {
                    name: "user_input".to_string(),
                    var_type: VarType::String,
                    required: true,
                }],
                Some(OutputContract {
                    min_length: Some(5),
                    max_length: None,
                    must_contain: vec!["Analyze".to_string()],
                    must_not_contain: Vec::new(),
                }),
            ),
        ),
        (
            "chat.messages".into(),
            make_slot(
                "chat.messages",
                vec![PromptKind::Messages],
                vec![VarSpec {
                    name: "persona".to_string(),
                    var_type: VarType::String,
                    required: true,
                }],
                None,
            ),
        ),
    ]);

    let raw_packs = HashMap::from([(
        "default".to_string(),
        PromptPack {
            name: "default".to_string(),
            extends: None,
            slots: HashMap::from([
                ("intent.analysis".into(), "intent/analysis".into()),
                ("chat.messages".into(), "chat/messages".into()),
            ]),
        },
    )]);
    let packs = HashMap::from([(
        "default".to_string(),
        ResolvedPack {
            name: "default".to_string(),
            resolved_slots: raw_packs["default"].slots.clone(),
            extends_chain: vec!["default".to_string()],
        },
    )]);

    PromptCatalog::from_parts(PromptCatalogParts {
        assets,
        slots,
        packs,
        raw_packs,
        resolver: PromptResolver {
            default_pack: "default".to_string(),
        },
        policies: Vec::new(),
        loaded_at: chrono::Utc::now(),
        renderer,
    })
}

#[test]
fn resolve_uses_manifest_hash_in_lineage() {
    let catalog = make_catalog();

    let resolved = catalog.resolve("intent.analysis", &PromptRenderOptions::default()).unwrap();

    assert_eq!(resolved.root.asset_id, "intent/analysis");
    assert_eq!(resolved.root.hash, Some("sha256:intent".to_string()));
    assert_eq!(resolved.pack, "default");
}

#[test]
fn render_slot_text_validates_and_returns_metadata() {
    let catalog = make_catalog();
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);

    let (rendered, metadata) = catalog
        .render_slot("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap();

    assert_eq!(rendered, RenderedPrompt::Text("Analyze: hello".to_string()));
    assert_eq!(metadata.slot, "intent.analysis");
    assert_eq!(metadata.pack, "default");
    assert_eq!(metadata.rendered_assets, vec!["intent/analysis"]);
    assert!(metadata.applied_policies.is_empty());
    assert_eq!(metadata.selection_reason, "default");
    assert!(metadata.input_validated);
    assert!(metadata.output_contract_validated);
    assert_eq!(metadata.root.hash, Some("sha256:intent".to_string()));
}

#[test]
fn render_slot_messages_returns_structured_messages() {
    let catalog = make_catalog();
    let vars = HashMap::from([("persona".to_string(), json!("a tutor"))]);

    let (rendered, metadata) = catalog
        .render_slot("chat.messages", &vars, &PromptRenderOptions::default())
        .unwrap();

    assert_eq!(
        rendered,
        RenderedPrompt::Messages(vec![PromptMessage {
            role: PromptRole::System,
            content: "You are a tutor.".to_string(),
        }])
    );
    assert!(metadata.applied_policies.is_empty());
}

#[test]
fn render_text_on_messages_slot_returns_kind_mismatch() {
    let catalog = make_catalog();
    let vars = HashMap::from([("persona".to_string(), json!("a tutor"))]);

    let err = catalog
        .render_text("chat.messages", &vars, &PromptRenderOptions::default())
        .unwrap_err();

    assert!(matches!(err, PromptError::KindMismatch { .. }));
}

#[test]
fn render_slot_applies_text_policies_and_records_metadata() {
    let mut catalog = make_catalog();
    catalog.policies.push(PromptPolicy::Preamble {
        name: "safety".to_string(),
        content: "[SAFETY NOTICE]".to_string(),
        position: PreamblePosition::Prepend,
    });
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);

    let (rendered, metadata) = catalog
        .render_slot("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap();

    assert_eq!(rendered.as_text(), Some("[SAFETY NOTICE]\nAnalyze: hello"));
    assert_eq!(metadata.applied_policies, vec!["safety".to_string()]);
}

#[test]
fn render_slot_fails_when_required_var_is_missing() {
    let catalog = make_catalog();

    let err = catalog
        .render_slot("intent.analysis", &HashMap::new(), &PromptRenderOptions::default())
        .unwrap_err();

    assert!(matches!(err, PromptError::SchemaValidationFailed(_)));
}

#[test]
fn render_slot_fails_when_output_contract_is_violated() {
    let mut catalog = make_catalog();
    catalog.slots.get_mut("intent.analysis").unwrap().output_contract = Some(OutputContract {
        min_length: None,
        max_length: None,
        must_contain: vec!["missing".to_string()],
        must_not_contain: Vec::new(),
    });
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);

    let err = catalog
        .render_slot("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap_err();

    assert!(matches!(err, PromptError::OutputContractViolation { .. }));
}

#[test]
fn render_slot_fails_when_slot_kind_does_not_accept_asset_kind() {
    let mut catalog = make_catalog();
    catalog.slots.get_mut("intent.analysis").unwrap().allowed_kinds = vec![PromptKind::Messages];
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);

    let err = catalog
        .render_slot("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap_err();

    assert!(matches!(err, PromptError::KindMismatch { .. }));
}

#[test]
fn render_slot_fails_when_asset_is_missing() {
    let mut catalog = make_catalog();
    catalog.assets.remove("intent/analysis");
    let vars = HashMap::from([("user_input".to_string(), json!("hello"))]);

    let err = catalog
        .render_slot("intent.analysis", &vars, &PromptRenderOptions::default())
        .unwrap_err();

    assert!(matches!(err, PromptError::AssetNotFound(_)));
}

#[test]
fn resolve_missing_slot_returns_slot_not_found() {
    let catalog = make_catalog();

    let err = catalog.resolve("missing.slot", &PromptRenderOptions::default()).unwrap_err();

    assert!(matches!(err, PromptError::SlotNotFound(_)));
}
