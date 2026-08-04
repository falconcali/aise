use super::*;

fn make_resolved_pack(name: &str, slots: &[(&str, &str)]) -> ResolvedPack {
    ResolvedPack {
        name: name.to_string(),
        resolved_slots: slots
            .iter()
            .map(|(slot_id, asset_ref)| ((*slot_id).into(), (*asset_ref).into()))
            .collect(),
        extends_chain: vec![name.to_string()],
    }
}

#[test]
fn select_pack_prefers_explicit_override() {
    let resolver = PromptResolver {
        default_pack: "default".to_string(),
    };
    let options = PromptRenderOptions::with_pack_override(Some("reasoning".to_string()));

    let (pack, reason) = resolver.select_pack(&options);

    assert_eq!(pack, "reasoning");
    assert_eq!(reason, "explicit_override");
}

#[test]
fn select_pack_falls_back_to_default() {
    let resolver = PromptResolver {
        default_pack: "default".to_string(),
    };

    let (pack, reason) = resolver.select_pack(&PromptRenderOptions::default());

    assert_eq!(pack, "default");
    assert_eq!(reason, "default");
}

#[test]
fn resolve_selection_returns_selected_pack_and_asset() {
    let resolver = PromptResolver {
        default_pack: "default".to_string(),
    };
    let packs = HashMap::from([(
        "default".to_string(),
        make_resolved_pack("default", &[("intent.analysis", "intent/analysis")]),
    )]);

    let selection = resolver
        .resolve_selection("intent.analysis", &PromptRenderOptions::default(), &packs)
        .unwrap();

    assert_eq!(selection.slot, "intent.analysis");
    assert_eq!(selection.pack, "default");
    assert_eq!(selection.asset_id, "intent/analysis");
    assert_eq!(selection.selection_reason, "default");
}

#[test]
fn resolve_selection_errors_when_pack_is_missing() {
    let resolver = PromptResolver {
        default_pack: "missing".to_string(),
    };

    let err = resolver
        .resolve_selection("intent.analysis", &PromptRenderOptions::default(), &HashMap::new())
        .unwrap_err();

    assert!(matches!(err, PromptError::PackNotFound(_)));
}

#[test]
fn resolve_selection_errors_when_slot_is_missing() {
    let resolver = PromptResolver {
        default_pack: "default".to_string(),
    };
    let packs = HashMap::from([("default".to_string(), make_resolved_pack("default", &[]))]);

    let err = resolver
        .resolve_selection("intent.analysis", &PromptRenderOptions::default(), &packs)
        .unwrap_err();

    assert!(matches!(err, PromptError::SlotNotFound(_)));
}

#[test]
fn parse_asset_ref_accepts_bare_asset_id() {
    assert_eq!(parse_asset_ref("intent/analysis").unwrap(), "intent/analysis");
}

#[test]
fn parse_asset_ref_rejects_empty_ref() {
    let err = parse_asset_ref("   ").unwrap_err();

    assert!(matches!(err, PromptError::AssetNotFound(_)));
    assert!(err.to_string().contains("cannot be empty"));
}

#[test]
fn parse_asset_ref_rejects_revision_suffix() {
    let err = parse_asset_ref("intent/analysis@1").unwrap_err();

    assert!(matches!(err, PromptError::AssetNotFound(_)));
    assert!(err.to_string().contains("bare `asset_id`"));
}
