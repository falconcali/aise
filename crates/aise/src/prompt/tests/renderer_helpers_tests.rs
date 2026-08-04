use super::*;
use crate::prompt::{
    asset::{CompiledPromptAsset, PromptAssetManifest},
    catalog::PromptCatalogParts,
    model::{AssetStatus, PromptKind},
    pack::{PromptPack, ResolvedPack},
    renderer::PromptRenderer,
    resolver::PromptResolver,
    slot::SlotSpec,
};
use serde_json::json;

fn make_catalog() -> Arc<PromptCatalog> {
    let mut renderer = PromptRenderer::new();
    renderer.add_template("default/message", "default: {{ topic }}").unwrap();
    renderer.add_template("reasoning/message", "reasoning: {{ topic }}").unwrap();

    let assets = HashMap::from([
        (
            "message/default".into(),
            CompiledPromptAsset {
                manifest: PromptAssetManifest {
                    asset_id: "message/default".into(),
                    kind: PromptKind::Text,
                    source_path: "files/default.md.j2".to_string(),
                    input_schema_ref: None,
                    output_contract_ref: None,
                    labels: HashMap::new(),
                    hash: None,
                    status: AssetStatus::Active,
                },
                source_anchor: "files/default.md.j2#message/default".to_string(),
                resolved_hash: "sha256:default".to_string(),
                template_name: "default/message".into(),
            },
        ),
        (
            "message/reasoning".into(),
            CompiledPromptAsset {
                manifest: PromptAssetManifest {
                    asset_id: "message/reasoning".into(),
                    kind: PromptKind::Text,
                    source_path: "files/reasoning.md.j2".to_string(),
                    input_schema_ref: None,
                    output_contract_ref: None,
                    labels: HashMap::new(),
                    hash: None,
                    status: AssetStatus::Active,
                },
                source_anchor: "files/reasoning.md.j2#message/reasoning".to_string(),
                resolved_hash: "sha256:reasoning".to_string(),
                template_name: "reasoning/message".into(),
            },
        ),
    ]);

    let slots = HashMap::from([(
        "demo.slot".into(),
        SlotSpec {
            slot_id: "demo.slot".into(),
            allowed_kinds: vec![PromptKind::Text],
            required: true,
            structured_output: false,
            output_contract_required: false,
            optimizable: false,
            allow_child_render: false,
            notes: None,
            vars: Vec::new(),
            output_contract: None,
        },
    )]);

    let raw_packs = HashMap::from([
        (
            "default".to_string(),
            PromptPack {
                name: "default".to_string(),
                extends: None,
                slots: HashMap::from([("demo.slot".into(), "message/default".into())]),
            },
        ),
        (
            "reasoning".to_string(),
            PromptPack {
                name: "reasoning".to_string(),
                extends: None,
                slots: HashMap::from([("demo.slot".into(), "message/reasoning".into())]),
            },
        ),
    ]);
    let packs = HashMap::from([
        (
            "default".to_string(),
            ResolvedPack {
                name: "default".to_string(),
                resolved_slots: raw_packs["default"].slots.clone(),
                extends_chain: vec!["default".to_string()],
            },
        ),
        (
            "reasoning".to_string(),
            ResolvedPack {
                name: "reasoning".to_string(),
                resolved_slots: raw_packs["reasoning"].slots.clone(),
                extends_chain: vec!["reasoning".to_string()],
            },
        ),
    ]);

    Arc::new(PromptCatalog::from_parts(PromptCatalogParts {
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
    }))
}

#[test]
fn try_render_slot_returns_none_without_catalog() {
    assert!(try_render_slot(None, "demo.slot", &HashMap::new()).is_none());
}

#[test]
fn try_render_slot_returns_rendered_text() {
    let catalog = make_catalog();
    let vars = HashMap::from([("topic".to_string(), json!("plan"))]);

    let rendered = try_render_slot(Some(&catalog), "demo.slot", &vars);

    assert_eq!(rendered, Some("default: plan".to_string()));
}

#[test]
fn try_render_slot_returns_none_on_render_error() {
    let catalog = make_catalog();

    let rendered = try_render_slot(Some(&catalog), "missing.slot", &HashMap::new());

    assert!(rendered.is_none());
}

#[test]
fn try_render_slot_with_options_respects_pack_override() {
    let catalog = make_catalog();
    let vars = HashMap::from([("topic".to_string(), json!("plan"))]);
    let options = PromptRenderOptions::with_pack_override(Some("reasoning".to_string()));

    let rendered = try_render_slot_with_options(Some(&catalog), "demo.slot", &vars, &options);

    assert_eq!(rendered, Some("reasoning: plan".to_string()));
}

#[test]
fn render_required_slot_returns_text() {
    let catalog = make_catalog();
    let vars = HashMap::from([("topic".to_string(), json!("plan"))]);

    let rendered = render_required_slot(Some(&catalog), "demo.slot", &vars).unwrap();

    assert_eq!(rendered, "default: plan");
}

#[test]
fn render_required_slot_fails_without_catalog() {
    let err = render_required_slot(None, "demo.slot", &HashMap::new()).unwrap_err();

    assert!(err.to_string().contains("PromptCatalog not loaded"));
}

#[test]
fn render_required_slot_with_options_wraps_error_context() {
    let catalog = make_catalog();

    let err = render_required_slot_with_options(
        Some(&catalog),
        "missing.slot",
        &HashMap::new(),
        &PromptRenderOptions::default(),
    )
    .unwrap_err();

    assert!(err.to_string().contains("failed to render required prompt slot `missing.slot`"));
}
