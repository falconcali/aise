use super::*;
use crate::core::CharacterCardId;
use crate::core::tests::{SNAKE_PACK_JSON, demo_pack, snake_pack};
use serde_json::json;

const LEGACY_V5_PACK_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/snake_pack.json"));

fn role(name: &str) -> serde_json::Value {
    json!({
        "name": name,
        "backstory": format!("{name} backstory"),
        "retrieval": format!("{name} hint"),
        "default_profile": { "personality": "calm" },
        "seed_memories": {
            "memory-1": { "content": "remembers the bridge", "retrieval": "bridge" }
        }
    })
}

fn pack_json() -> serde_json::Value {
    json!({
        "spec": "aise_story_v1",
        "pack_id": "pack-1",
        "meta": { "creator": "aise", "version": "1.0.0" },
        "profile": {
            "title": "白蛇传",
            "description": null,
            "genres": ["myth"],
            "point_of_view": "第三人称",
            "tense": "过去时"
        },
        "roles": { "xu_xian": role("许仙"), "bai_suzhen": role("白素贞") },
        "play": { "playable_roles": ["xu_xian"] },
        "opening": "opening",
        "seed_facts": { "fact-1": { "content": "西湖有白蛇", "retrieval": "白蛇" } }
    })
}

fn pack(value: serde_json::Value) -> StoryPack {
    serde_json::from_value(value).expect("test pack should deserialize")
}

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn card_ref() -> CharacterCardRef {
    CharacterCardRef {
        card_id: CharacterCardId::try_new("card-1").expect("test card id should be valid"),
        version: SemanticVersion::try_new("1.0.0").expect("test version should be valid"),
        digest: Sha256Digest::try_new("digest").expect("test digest should be valid"),
    }
}

#[test]
fn example_story_packs_import() {
    let snake = snake_pack();
    let demo = demo_pack();

    assert!(snake.play.playable_roles.contains(&role_id("xu_xian")));
    assert_eq!(snake.narrative.as_ref().map(|graph| graph.nodes.len()), Some(14));
    assert!(demo.play.playable_roles.contains(&role_id("protagonist")));
}

#[test]
fn legacy_v5_pack_is_rejected_as_malformed() {
    assert!(serde_json::from_str::<StoryPack>(LEGACY_V5_PACK_JSON).is_err());
}

#[test]
fn digest_is_stable_hex_and_content_sensitive() {
    let digest = snake_pack().pack_ref().expect("pack ref should compute").digest;
    let reparsed = serde_json::from_str::<StoryPack>(SNAKE_PACK_JSON)
        .expect("snake pack should import")
        .pack_ref()
        .expect("pack ref should compute")
        .digest;
    let other = demo_pack().pack_ref().expect("pack ref should compute").digest;

    assert_eq!(digest, reparsed);
    assert_ne!(digest, other);
    assert_eq!(digest.as_str().len(), 64);
    assert!(digest.as_str().chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn pack_ref_pins_pack_id_version_and_digest() {
    let pack = snake_pack();

    let pack_ref = pack.pack_ref().expect("pack ref should compute");

    assert_eq!(pack_ref.pack_id, pack.pack_id);
    assert_eq!(pack_ref.version, pack.meta.version);
    assert_eq!(pack_ref.digest, pack.pack_ref().expect("pack ref should compute").digest);
}

#[test]
fn deserializes_pack_with_keyed_seeds_and_optional_activation() {
    let pack = pack(pack_json());

    assert_eq!(pack.roles.len(), 2);
    assert!(pack.play.playable_roles.contains(&role_id("xu_xian")));
    assert!(pack.seed_facts.contains_key(&FactId::try_new("fact-1").expect("valid fact id")));
    let memories = &pack.roles[&role_id("xu_xian")].seed_memories;
    assert!(memories.contains_key(&crate::core::MemoryId::try_new("memory-1").expect("valid memory id")));
}

#[test]
fn rejects_pack_without_pack_id() {
    let mut value = pack_json();
    value.as_object_mut().expect("pack json is an object").remove("pack_id");

    assert!(serde_json::from_value::<StoryPack>(value).is_err());
}

#[test]
fn rejects_empty_pack_id() {
    let mut value = pack_json();
    value["pack_id"] = json!(" ");

    assert!(serde_json::from_value::<StoryPack>(value).is_err());
}

#[test]
fn rejects_empty_role_id_key() {
    let mut value = pack_json();
    value["roles"] = json!({ "": role("nobody") });

    assert!(serde_json::from_value::<StoryPack>(value).is_err());
}

#[test]
fn rejects_character_name_inside_role_profile() {
    let mut value = pack_json();
    value["roles"]["xu_xian"]["default_profile"]["name"] = json!("someone else");

    assert!(serde_json::from_value::<StoryPack>(value).is_err());
}

#[test]
fn valid_pack_passes_validation() {
    assert!(pack(pack_json()).validate().is_ok());
}

#[test]
fn empty_playable_roles_are_rejected() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!([]);

    assert!(matches!(pack(value).validate(), Err(StoryError::NoPlayableRole)));
}

#[test]
fn playable_role_must_be_defined() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!(["ghost"]);

    assert!(matches!(
        pack(value).validate(),
        Err(StoryError::UnknownRole { field: "play.playable_roles", role_id }) if role_id == "ghost"
    ));
}

#[test]
fn pack_with_valid_narrative_passes_validation() {
    let mut value = pack_json();
    value["narrative"] = json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "activate_when": { "type": "story_started" },
            "complete_when": { "type": "turn_reaches", "turn": 3 },
            "constraints": [{ "forbid": "The ritual truth is revealed.", "window": "while_active" }],
            "terminal": true
        }
    }});

    assert!(pack(value).validate().is_ok());
}

#[test]
fn pack_with_invalid_narrative_is_rejected() {
    let mut value = pack_json();
    value["narrative"] = json!({ "nodes": {
        "arrival": {
            "title": "Arrival",
            "activate_when": { "type": "node_is", "node_id": "ghost", "status": "active" },
            "complete_when": { "type": "story_started" }
        }
    }});

    assert!(matches!(
        pack(value).validate(),
        Err(StoryError::Narrative(crate::core::NarrativeError::UnknownNode { .. }))
    ));
}

#[test]
fn resolve_cast_fills_unbound_roles_with_default_actor() {
    let pack = pack(pack_json());
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActor::Card(card_ref()))]);

    let resolved = pack.resolve_cast(&role_id("xu_xian"), cast).expect("cast should resolve");

    assert!(matches!(resolved[&role_id("xu_xian")], RoleActor::Card(_)));
    assert!(matches!(resolved[&role_id("bai_suzhen")], RoleActor::Default));
}

#[test]
fn resolve_cast_rejects_non_playable_player_role() {
    let pack = pack(pack_json());

    assert!(matches!(
        pack.resolve_cast(&role_id("bai_suzhen"), BTreeMap::new()),
        Err(StoryError::RoleNotPlayable { role_id }) if role_id == "bai_suzhen"
    ));
}

#[test]
fn resolve_cast_rejects_unknown_cast_role() {
    let pack = pack(pack_json());
    let cast = BTreeMap::from([(role_id("ghost"), RoleActor::Default)]);

    assert!(matches!(
        pack.resolve_cast(&role_id("xu_xian"), cast),
        Err(StoryError::UnknownRole { field: "cast", role_id }) if role_id == "ghost"
    ));
}
