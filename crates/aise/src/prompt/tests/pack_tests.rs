use super::*;

fn make_pack(name: &str, extends: Option<&str>, slots: &[(&str, &str)]) -> PromptPack {
    PromptPack {
        name: name.to_string(),
        extends: extends.map(str::to_string),
        slots: slots
            .iter()
            .map(|(slot_id, asset_ref)| ((*slot_id).into(), (*asset_ref).into()))
            .collect(),
    }
}

#[test]
fn single_pack_resolves() {
    let packs = HashMap::from([(
        "base".to_string(),
        make_pack("base", None, &[("intent.analysis", "intent/analysis")]),
    )]);

    let resolved = resolve_pack("base", &packs).unwrap();

    assert_eq!(resolved.name, "base");
    assert_eq!(resolved.extends_chain, vec!["base"]);
    assert_eq!(resolved.resolved_slots["intent.analysis"], "intent/analysis");
}

#[test]
fn child_pack_overrides_parent_slots() {
    let packs = HashMap::from([
        (
            "base".to_string(),
            make_pack(
                "base",
                None,
                &[("intent.analysis", "intent/analysis"), ("summary.main", "summary/main")],
            ),
        ),
        (
            "child".to_string(),
            make_pack("child", Some("base"), &[("intent.analysis", "intent/analysis.variant")]),
        ),
    ]);

    let resolved = resolve_pack("child", &packs).unwrap();

    assert_eq!(resolved.extends_chain, vec!["child", "base"]);
    assert_eq!(resolved.resolved_slots["intent.analysis"], "intent/analysis.variant");
    assert_eq!(resolved.resolved_slots["summary.main"], "summary/main");
}

#[test]
fn missing_parent_pack_fails() {
    let packs = HashMap::from([("child".to_string(), make_pack("child", Some("missing"), &[]))]);

    let err = resolve_pack("child", &packs).unwrap_err();

    assert!(matches!(err, PromptError::PackNotFound(_)));
}

#[test]
fn inheritance_cycle_fails() {
    let packs = HashMap::from([
        ("a".to_string(), make_pack("a", Some("b"), &[])),
        ("b".to_string(), make_pack("b", Some("a"), &[])),
    ]);

    let err = resolve_pack("a", &packs).unwrap_err();

    assert!(matches!(err, PromptError::InheritanceCycleOrDepthExceeded(_)));
}

#[test]
fn chain_at_depth_limit_still_resolves() {
    let packs = HashMap::from([
        ("base".to_string(), make_pack("base", None, &[("slot", "base/asset")])),
        (
            "level1".to_string(),
            make_pack("level1", Some("base"), &[("slot", "level1/asset")]),
        ),
        (
            "level2".to_string(),
            make_pack("level2", Some("level1"), &[("slot", "level2/asset")]),
        ),
        (
            "level3".to_string(),
            make_pack("level3", Some("level2"), &[("slot", "level3/asset")]),
        ),
    ]);

    let resolved = resolve_pack("level3", &packs).unwrap();

    assert_eq!(resolved.extends_chain, vec!["level3", "level2", "level1", "base"]);
    assert_eq!(resolved.resolved_slots["slot"], "level3/asset");
}

#[test]
fn chain_beyond_depth_limit_fails() {
    let packs = HashMap::from([
        ("base".to_string(), make_pack("base", None, &[])),
        ("level1".to_string(), make_pack("level1", Some("base"), &[])),
        ("level2".to_string(), make_pack("level2", Some("level1"), &[])),
        ("level3".to_string(), make_pack("level3", Some("level2"), &[])),
        ("level4".to_string(), make_pack("level4", Some("level3"), &[])),
    ]);

    let err = resolve_pack("level4", &packs).unwrap_err();

    assert!(matches!(err, PromptError::InheritanceCycleOrDepthExceeded(_)));
    assert!(err.to_string().contains("max depth"));
}
