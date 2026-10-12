use super::*;
use crate::core::{CharacterCardId, CharacterCardRef, Sha256Digest};
use serde_json::json;

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
fn deserializes_pack_with_keyed_seeds_and_optional_activation() {
    let pack = pack(pack_json());

    assert_eq!(pack.roles.len(), 2);
    assert!(pack.play.playable_roles.contains(&role_id("xu_xian")));
    assert!(pack.seed_facts.contains_key(&FactId::try_new("fact-1").expect("valid fact id")));
    let memories = &pack.roles[&role_id("xu_xian")].seed_memories;
    assert!(memories.contains_key(&crate::core::MemoryId::try_new("memory-1").expect("valid memory id")));
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
        Err(AssetError::RoleNotPlayable { role_id }) if role_id == "bai_suzhen"
    ));
}

#[test]
fn resolve_cast_rejects_unknown_cast_role() {
    let pack = pack(pack_json());
    let cast = BTreeMap::from([(role_id("ghost"), RoleActor::Default)]);

    assert!(matches!(
        pack.resolve_cast(&role_id("xu_xian"), cast),
        Err(AssetError::UnknownRole { field: "cast", role_id }) if role_id == "ghost"
    ));
}
