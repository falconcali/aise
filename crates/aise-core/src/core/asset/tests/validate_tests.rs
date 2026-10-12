use super::*;
use serde_json::json;

fn pack_json() -> serde_json::Value {
    json!({
        "spec": "aise_story_v1",
        "meta": { "creator": "aise", "version": "1.0.0" },
        "profile": {
            "title": "白蛇传",
            "description": null,
            "point_of_view": "第三人称",
            "tense": "过去时"
        },
        "roles": {
            "xu_xian": {
                "name": "许仙",
                "backstory": "药铺伙计",
                "retrieval": "许仙",
                "default_profile": {},
                "seed_memories": {
                    "memory-1": { "content": "断桥借伞", "retrieval": "借伞" }
                }
            }
        },
        "play": { "playable_roles": ["xu_xian"] },
        "opening": "opening",
        "seed_facts": { "fact-1": { "content": "西湖有白蛇", "retrieval": "白蛇" } }
    })
}

fn pack(value: serde_json::Value) -> StoryPack {
    serde_json::from_value(value).expect("test pack should deserialize")
}

fn card(name: &str) -> CharacterCard {
    serde_json::from_value(json!({
        "spec": "aise_char_v1",
        "card_id": "card-1",
        "name": name,
        "meta": { "creator": "aise", "version": "1.0.0" },
        "profile": { "speaking_style": "gentle" }
    }))
    .expect("test card should deserialize")
}

#[test]
fn valid_pack_passes_validation() {
    assert!(pack(pack_json()).validate(&AssetConfig::default()).is_ok());
}

#[test]
fn empty_playable_roles_are_rejected() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!([]);

    assert!(matches!(
        pack(value).validate(&AssetConfig::default()),
        Err(AssetError::NoPlayableRole)
    ));
}

#[test]
fn playable_role_must_be_defined() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!(["ghost"]);

    assert!(matches!(
        pack(value).validate(&AssetConfig::default()),
        Err(AssetError::UnknownRole { field: "play.playable_roles", role_id }) if role_id == "ghost"
    ));
}

#[test]
fn opening_longer_than_limit_is_rejected() {
    let config = AssetConfig {
        max_opening_bytes: 4,
        ..AssetConfig::default()
    };

    assert!(matches!(
        pack(pack_json()).validate(&config),
        Err(AssetError::TextTooLong {
            field: "opening",
            actual: 7,
            maximum: 4
        })
    ));
}

#[test]
fn seed_memory_error_reports_role_and_memory_ids() {
    let mut value = pack_json();
    value["roles"]["xu_xian"]["seed_memories"]["memory-1"]["content"] = json!("x".repeat(16));
    let config = AssetConfig {
        max_text_bytes: 12,
        ..AssetConfig::default()
    };

    let error = pack(value).validate(&config).expect_err("long memory should fail");

    let AssetError::InvalidEntry {
        section: "roles",
        id: role_id,
        source,
    } = error
    else {
        panic!("expected role entry error, got {error:?}");
    };
    assert_eq!(role_id, "xu_xian");
    assert!(matches!(
        *source,
        AssetError::InvalidEntry { section: "seed_memories", ref id, ref source }
            if id == "memory-1" && matches!(**source, AssetError::TextTooLong { field: "content", .. })
    ));
}

#[test]
fn role_backstory_error_names_backstory_field() {
    let config = AssetConfig {
        max_text_bytes: 4,
        ..AssetConfig::default()
    };

    assert!(matches!(
        pack(pack_json()).validate(&config),
        Err(AssetError::InvalidEntry { section: "roles", source, .. })
            if matches!(*source, AssetError::TextTooLong { field: "backstory", .. })
    ));
}

#[test]
fn too_many_seed_facts_are_rejected() {
    let config = AssetConfig {
        max_seed_entries: 0,
        ..AssetConfig::default()
    };

    assert!(matches!(
        pack(pack_json()).validate(&config),
        Err(AssetError::TooManyItems {
            field: "seed_facts",
            actual: 1,
            maximum: 0
        })
    ));
}

#[test]
fn character_card_name_longer_than_limit_is_rejected() {
    let config = AssetConfig {
        max_label_bytes: 4,
        ..AssetConfig::default()
    };

    assert!(card("白素贞").validate(&AssetConfig::default()).is_ok());
    assert!(matches!(
        card("白素贞").validate(&config),
        Err(AssetError::TextTooLong { field: "name", .. })
    ));
}
