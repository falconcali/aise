use super::*;
use crate::core::tests::{SNAKE_PACK_JSON, demo_pack, snake_pack};
use crate::core::{MAX_ALIASES_PER_ROLE, MAX_MEMORIES_PER_ROLE, MAX_ROLE_CATALOG_BYTES, MAX_ROLES_PER_STORY};
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
fn legacy_story_pack_digest_matches_golden_value() {
    assert_eq!(
        snake_pack().pack_ref().expect("pack ref should compute").digest.as_str(),
        "aa5d334278c8250ce863e002eb4eb099e1cf93ada90030933833787080fa8257"
    );
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
fn legacy_story_pack_round_trip_preserves_canonical_bytes() {
    let pack = serde_json::from_str::<StoryPack>(SNAKE_PACK_JSON).expect("legacy pack should import");
    let canonical = serde_json::to_vec(&pack).expect("legacy pack should serialize");
    let reparsed = serde_json::from_slice::<StoryPack>(&canonical).expect("canonical pack should import");

    assert_eq!(
        serde_json::to_vec(&reparsed).expect("reparsed pack should serialize"),
        canonical
    );
}

#[test]
fn legacy_story_pack_rejects_unknown_and_duplicate_fields() {
    let mut unknown = pack_json();
    unknown["unknown"] = json!(true);
    let duplicate = r#"{"spec":"aise_story_v1","spec":"aise_story_v1","pack_id":"pack-1","meta":{"creator":"aise","version":"1.0.0"},"profile":{"title":"title","description":null,"point_of_view":"third","tense":"past"},"roles":{},"play":{"playable_roles":[]},"opening":"opening"}"#;

    assert!(serde_json::from_value::<StoryPack>(unknown).is_err());
    assert!(serde_json::from_str::<StoryPack>(duplicate).is_err());
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
    assert!(pack(pack_json()).validate_play().is_ok());
    assert!(pack(pack_json()).validate_narrative().is_ok());
    assert!(pack(pack_json()).validate_roles().is_ok());
}

#[test]
fn empty_playable_roles_are_rejected() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!([]);

    assert!(matches!(pack(value).validate_play(), Err(StoryError::NoPlayableRole)));
}

#[test]
fn playable_role_must_be_defined() {
    let mut value = pack_json();
    value["play"]["playable_roles"] = json!(["ghost"]);

    assert!(matches!(
        pack(value).validate_play(),
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

    assert!(pack(value.clone()).validate_play().is_ok());
    assert!(pack(value).validate_narrative().is_ok());
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
        pack(value).validate_narrative(),
        Err(crate::core::NarrativeError::UnknownNode { .. })
    ));
}

fn text(value: impl Into<String>) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn pack_with_role_count(count: usize) -> StoryPack {
    let mut pack = pack(pack_json());
    let role = pack.roles.values().next().expect("fixture role").clone();
    pack.roles = (0..count)
        .map(|index| (role_id(&format!("role-{index:03}")), role.clone()))
        .collect();
    pack
}

fn single_role_pack() -> StoryPack {
    let mut pack = pack(pack_json());
    let role = pack.roles.values().next().expect("fixture role").clone();
    pack.roles = BTreeMap::from([(role_id("r"), role)]);
    pack
}

fn pack_with_catalog_bytes(size: usize) -> StoryPack {
    let mut pack = single_role_pack();
    let role_id = pack.roles.keys().next().expect("fixture role").clone();
    let role = pack.roles.get_mut(&role_id).expect("fixture role");
    role.spec.alias.clear();
    let fixed_bytes = role_id.as_str().len() + role.spec.retrieval.as_str().len();
    role.spec.name = text("x".repeat(size - fixed_bytes));
    pack
}

fn pack_with_seed_memory_count(count: usize) -> StoryPack {
    let mut pack = single_role_pack();
    let role = pack.roles.values_mut().next().expect("fixture role");
    let memory = role.seed_memories.values().next().expect("fixture memory").clone();
    role.seed_memories = (0..count)
        .map(|index| {
            (
                crate::core::MemoryId::try_new(format!("memory-{index:03}")).expect("valid memory id"),
                memory.clone(),
            )
        })
        .collect();
    pack
}

#[test]
fn role_count_at_limit_is_accepted() {
    assert!(pack_with_role_count(MAX_ROLES_PER_STORY).validate_roles().is_ok());
}

#[test]
fn role_count_over_limit_is_rejected() {
    assert!(matches!(
        pack_with_role_count(MAX_ROLES_PER_STORY + 1).validate_roles(),
        Err(StoryError::TooManyRoles {
            actual,
            maximum: MAX_ROLES_PER_STORY
        }) if actual == MAX_ROLES_PER_STORY + 1
    ));
}

#[test]
fn alias_count_at_limit_is_accepted() {
    let mut pack = single_role_pack();
    pack.roles.values_mut().next().expect("fixture role").spec.alias = vec![text("alias"); MAX_ALIASES_PER_ROLE];

    assert!(pack.validate_roles().is_ok());
}

#[test]
fn alias_count_over_limit_is_rejected() {
    let mut pack = single_role_pack();
    pack.roles.values_mut().next().expect("fixture role").spec.alias = vec![text("alias"); MAX_ALIASES_PER_ROLE + 1];

    assert!(matches!(
        pack.validate_roles(),
        Err(StoryError::TooManyRoleAliases {
            actual,
            maximum: MAX_ALIASES_PER_ROLE,
            ..
        }) if actual == MAX_ALIASES_PER_ROLE + 1
    ));
}

#[test]
fn role_catalog_bytes_at_limit_are_accepted() {
    assert!(pack_with_catalog_bytes(MAX_ROLE_CATALOG_BYTES).validate_roles().is_ok());
}

#[test]
fn role_catalog_bytes_over_limit_are_rejected() {
    assert!(matches!(
        pack_with_catalog_bytes(MAX_ROLE_CATALOG_BYTES + 1).validate_roles(),
        Err(StoryError::RoleCatalogTooLarge {
            actual,
            maximum: MAX_ROLE_CATALOG_BYTES
        }) if actual == MAX_ROLE_CATALOG_BYTES + 1
    ));
}

#[test]
fn seed_memory_count_at_limit_is_accepted() {
    assert!(pack_with_seed_memory_count(MAX_MEMORIES_PER_ROLE).validate_roles().is_ok());
}

#[test]
fn seed_memory_count_over_limit_is_rejected() {
    assert!(matches!(
        pack_with_seed_memory_count(MAX_MEMORIES_PER_ROLE + 1).validate_roles(),
        Err(StoryError::TooManyRoleMemories {
            actual,
            maximum: MAX_MEMORIES_PER_ROLE,
            ..
        }) if actual == MAX_MEMORIES_PER_ROLE + 1
    ));
}
