use super::*;
use crate::core::{CharacterCardId, RoleId};
use crate::test_support::{DEMO_PACK_JSON, SNAKE_PACK_JSON, XU_XIAN_CARD_JSON, demo_pack, snake_pack, xu_xian_card};

const LEGACY_V5_PACK_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack.json"));

#[test]
fn example_story_packs_import() {
    let snake = snake_pack();
    let demo = demo_pack();

    assert!(
        snake
            .play
            .playable_roles
            .contains(&RoleId::try_new("xu_xian").expect("valid role id"))
    );
    assert_eq!(snake.narrative.as_ref().map(|graph| graph.nodes.len()), Some(14));
    assert!(
        demo.play
            .playable_roles
            .contains(&RoleId::try_new("protagonist").expect("valid role id"))
    );
}

#[test]
fn example_character_card_imports() {
    let card = xu_xian_card();

    assert_eq!(
        card.card_id,
        CharacterCardId::try_new("xu_xian_bold_physician").expect("valid card id")
    );
}

#[test]
fn legacy_v5_pack_is_rejected_as_malformed() {
    assert!(matches!(
        StoryPack::from_json(LEGACY_V5_PACK_JSON, &AssetConfig::default()),
        Err(AssetError::MalformedDocument {
            asset: "story_pack",
            ..
        })
    ));
}

#[test]
fn invalid_json_is_rejected_as_malformed() {
    assert!(matches!(
        CharacterCard::from_json("{", &AssetConfig::default()),
        Err(AssetError::MalformedDocument {
            asset: "character_card",
            ..
        })
    ));
}

#[test]
fn oversized_document_is_rejected_before_parsing() {
    let config = AssetConfig {
        max_document_bytes: 16,
        ..AssetConfig::default()
    };

    assert!(matches!(
        StoryPack::from_json(DEMO_PACK_JSON, &config),
        Err(AssetError::DocumentTooLarge {
            asset: "story_pack",
            maximum: 16,
            ..
        })
    ));
}

#[test]
fn imported_document_is_validated_against_limits() {
    let config = AssetConfig {
        max_label_bytes: 4,
        ..AssetConfig::default()
    };

    assert!(matches!(
        CharacterCard::from_json(XU_XIAN_CARD_JSON, &config),
        Err(AssetError::TextTooLong { field: "name", .. })
    ));
}

#[test]
fn digest_is_stable_hex_and_content_sensitive() {
    let digest = snake_pack().digest().expect("digest should compute");
    let reparsed = StoryPack::from_json(SNAKE_PACK_JSON, &AssetConfig::default())
        .expect("snake pack should import")
        .digest()
        .expect("digest should compute");
    let other = demo_pack().digest().expect("digest should compute");

    assert_eq!(digest, reparsed);
    assert_ne!(digest, other);
    assert_eq!(digest.as_str().len(), 64);
    assert!(digest.as_str().chars().all(|ch| ch.is_ascii_hexdigit()));
}
