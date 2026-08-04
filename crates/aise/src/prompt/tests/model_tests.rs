use super::*;

#[test]
fn prompt_kind_serde_roundtrip() {
    let kinds = [
        PromptKind::Text,
        PromptKind::Messages,
        PromptKind::Fragment,
        PromptKind::FewShot,
    ];

    for kind in kinds {
        let json = serde_json::to_string(&kind).unwrap();
        let roundtrip: PromptKind = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, kind);
    }
}

#[test]
fn prompt_kind_display_uses_snake_case() {
    assert_eq!(PromptKind::Text.to_string(), "text");
    assert_eq!(PromptKind::Messages.to_string(), "messages");
    assert_eq!(PromptKind::Fragment.to_string(), "fragment");
    assert_eq!(PromptKind::FewShot.to_string(), "few_shot");
}

#[test]
fn asset_status_serde_roundtrip() {
    let statuses = [
        AssetStatus::Active,
        AssetStatus::Deprecated,
        AssetStatus::Archived,
        AssetStatus::Candidate,
    ];

    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let roundtrip: AssetStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, status);
    }
}

#[test]
fn slot_id_and_asset_ref_roundtrip_as_strings() {
    let slot_id = SlotId::new("intent.analysis");
    let asset_ref = AssetRef::new("intent/analysis");

    assert_eq!(slot_id.to_string(), "intent.analysis");
    assert_eq!(asset_ref.to_string(), "intent/analysis");

    let slot_json = serde_json::to_string(&slot_id).unwrap();
    let asset_json = serde_json::to_string(&asset_ref).unwrap();

    assert_eq!(slot_json, "\"intent.analysis\"");
    assert_eq!(asset_json, "\"intent/analysis\"");

    let roundtrip_slot: SlotId = serde_json::from_str(&slot_json).unwrap();
    let roundtrip_asset: AssetRef = serde_json::from_str(&asset_json).unwrap();

    assert_eq!(roundtrip_slot, slot_id);
    assert_eq!(roundtrip_asset, asset_ref);
}

#[test]
fn rendered_prompt_text_accessors_match_variant() {
    let rendered = RenderedPrompt::Text("hello".to_string());

    assert_eq!(rendered.as_text(), Some("hello"));
    assert!(rendered.as_messages().is_none());
}

#[test]
fn rendered_prompt_messages_accessors_match_variant() {
    let rendered = RenderedPrompt::Messages(vec![PromptMessage {
        role: PromptRole::System,
        content: "You are helpful.".to_string(),
    }]);

    assert!(rendered.as_text().is_none());
    assert_eq!(rendered.as_messages().unwrap()[0].role, PromptRole::System);
}
