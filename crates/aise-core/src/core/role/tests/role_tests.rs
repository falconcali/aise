use super::*;
use crate::core::{ActivationRule, CoreError};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;

fn text(value: impl Into<String>) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

fn role_id(value: impl Into<String>) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn memory_id(value: impl Into<String>) -> MemoryId {
    MemoryId::try_new(value).expect("test memory id should be valid")
}

fn retrieval(value: impl Into<String>) -> RetrievalHint {
    RetrievalHint::try_new(value).expect("test retrieval should be valid")
}

fn profile() -> CharacterProfile {
    CharacterProfile {
        appearance: None,
        personality: None,
        speaking_style: None,
        dialogue_examples: Vec::new(),
    }
}

fn role_spec() -> RoleSpec {
    RoleSpec {
        name: text("role"),
        alias: Vec::new(),
        backstory: text("backstory"),
        retrieval: retrieval("retrieval"),
    }
}

fn memory_body() -> MemoryBody {
    MemoryBody {
        content: text("memory"),
        activation: ActivationRule::default(),
        retrieval: retrieval("memory"),
    }
}

fn seed_role() -> SeedRole {
    SeedRole {
        spec: role_spec(),
        default_profile: profile(),
        seed_memories: BTreeMap::new(),
    }
}

fn runtime_role(id: RoleId) -> RoleEntry {
    RoleEntry {
        id,
        origin: EntryOrigin::Seed,
        spec: role_spec(),
        profile: profile(),
        memories: BTreeMap::new(),
    }
}

fn runtime_roles_with_state_bytes(size: usize) -> BTreeMap<RoleId, RoleEntry> {
    let id = role_id("r");
    let mut roles = BTreeMap::from([(id.clone(), runtime_role(id.clone()))]);
    roles.get_mut(&id).expect("fixture role").spec.name = text("");
    roles.get_mut(&id).expect("fixture role").spec.alias.clear();
    roles.get_mut(&id).expect("fixture role").spec.backstory = text("");
    roles.get_mut(&id).expect("fixture role").spec.retrieval = retrieval("r");
    let base_size = serde_json::to_vec(&roles).expect("runtime roles should serialize").len();
    roles.get_mut(&id).expect("fixture role").spec.backstory = text("x".repeat(size - base_size));
    assert_eq!(serde_json::to_vec(&roles).expect("runtime roles should serialize").len(), size);
    roles
}

#[test]
fn role_spec_defaults_alias_and_rejects_unknown_fields() {
    let value = json!({
        "name": "role",
        "backstory": "backstory",
        "retrieval": "retrieval"
    });
    let spec: RoleSpec = serde_json::from_value(value.clone()).expect("role spec should deserialize");
    assert!(spec.alias.is_empty());

    let mut unknown = value;
    unknown["unknown"] = json!(true);
    assert!(serde_json::from_value::<RoleSpec>(unknown).is_err());
}

#[test]
fn role_spec_rejects_duplicate_fields() {
    let json = r#"{"name":"first","name":"second","backstory":"backstory","retrieval":"retrieval"}"#;

    assert!(serde_json::from_str::<RoleSpec>(json).is_err());
}

#[test]
fn seed_role_serializes_flat_in_legacy_field_order() {
    let serialized = serde_json::to_string(&seed_role()).expect("seed role should serialize");

    assert_eq!(
        serialized,
        r#"{"name":"role","alias":[],"backstory":"backstory","retrieval":"retrieval","default_profile":{"appearance":null,"personality":null,"speaking_style":null,"dialogue_examples":[]},"seed_memories":{}}"#
    );
}

#[test]
fn seed_role_defaults_alias_and_memories() {
    let json = r#"{"name":"role","backstory":"backstory","retrieval":"retrieval","default_profile":{"appearance":null,"personality":null,"speaking_style":null}}"#;
    let role: SeedRole = serde_json::from_str(json).expect("seed role should deserialize");

    assert!(role.spec.alias.is_empty());
    assert!(role.seed_memories.is_empty());
}

#[test]
fn seed_role_rejects_unknown_and_duplicate_fields() {
    let unknown = r#"{"name":"role","backstory":"backstory","retrieval":"retrieval","default_profile":{"appearance":null,"personality":null,"speaking_style":null},"unknown":true}"#;
    let duplicate = r#"{"name":"first","name":"second","backstory":"backstory","retrieval":"retrieval","default_profile":{"appearance":null,"personality":null,"speaking_style":null}}"#;

    assert!(serde_json::from_str::<SeedRole>(unknown).is_err());
    assert!(serde_json::from_str::<SeedRole>(duplicate).is_err());
}

#[test]
fn seed_role_round_trips_without_changing_flat_shape() {
    let role = seed_role();
    let bytes = serde_json::to_vec(&role).expect("seed role should serialize");
    let reparsed: SeedRole = serde_json::from_slice(&bytes).expect("seed role should deserialize");

    assert_eq!(
        serde_json::to_vec(&reparsed).expect("reparsed seed role should serialize"),
        bytes
    );
}

#[test]
fn seed_role_count_at_limit_is_accepted() {
    let roles = (0..MAX_ROLES_PER_STORY)
        .map(|index| (role_id(format!("role-{index:03}")), seed_role()))
        .collect();

    assert!(validate_seed_roles(&roles).is_ok());
}

#[test]
fn seed_role_count_over_limit_is_rejected() {
    let roles = (0..=MAX_ROLES_PER_STORY)
        .map(|index| (role_id(format!("role-{index:03}")), seed_role()))
        .collect();

    assert!(matches!(
        validate_seed_roles(&roles),
        Err(StoryError::TooManyRoles {
            actual,
            maximum: MAX_ROLES_PER_STORY
        }) if actual == MAX_ROLES_PER_STORY + 1
    ));
}

#[test]
fn alias_count_at_limit_is_accepted() {
    let id = role_id("role");
    let mut spec = role_spec();
    spec.alias = vec![text("alias"); MAX_ALIASES_PER_ROLE];

    assert!(validate_role_spec(&id, &spec).is_ok());
}

#[test]
fn alias_count_over_limit_is_rejected() {
    let id = role_id("role");
    let mut spec = role_spec();
    spec.alias = vec![text("alias"); MAX_ALIASES_PER_ROLE + 1];

    assert!(matches!(
        validate_role_spec(&id, &spec),
        Err(StoryError::TooManyRoleAliases {
            role_id,
            actual,
            maximum: MAX_ALIASES_PER_ROLE
        }) if role_id == id && actual == MAX_ALIASES_PER_ROLE + 1
    ));
}

#[test]
fn role_catalog_bytes_at_limit_are_accepted() {
    let id = role_id("r");
    let mut role = runtime_role(id.clone());
    role.spec.alias.clear();
    role.spec.retrieval = retrieval("r");
    role.spec.name = text("x".repeat(MAX_ROLE_CATALOG_BYTES - id.as_str().len() - 1));
    let roles = BTreeMap::from([(id, role)]);

    assert!(validate_runtime_roles(&roles).is_ok());
}

#[test]
fn role_catalog_bytes_over_limit_are_rejected() {
    let id = role_id("r");
    let mut role = runtime_role(id.clone());
    role.spec.alias.clear();
    role.spec.retrieval = retrieval("r");
    role.spec.name = text("x".repeat(MAX_ROLE_CATALOG_BYTES - id.as_str().len()));
    let roles = BTreeMap::from([(id, role)]);

    assert!(matches!(
        validate_runtime_roles(&roles),
        Err(StoryError::RoleCatalogTooLarge {
            actual,
            maximum: MAX_ROLE_CATALOG_BYTES
        }) if actual == MAX_ROLE_CATALOG_BYTES + 1
    ));
}

#[test]
fn seed_memory_count_at_limit_is_accepted() {
    let id = role_id("role");
    let mut role = seed_role();
    role.seed_memories = (0..MAX_MEMORIES_PER_ROLE)
        .map(|index| (memory_id(format!("memory-{index:03}")), memory_body()))
        .collect();
    let roles = BTreeMap::from([(id, role)]);

    assert!(validate_seed_roles(&roles).is_ok());
}

#[test]
fn seed_memory_count_over_limit_is_rejected() {
    let id = role_id("role");
    let mut role = seed_role();
    role.seed_memories = (0..=MAX_MEMORIES_PER_ROLE)
        .map(|index| (memory_id(format!("memory-{index:03}")), memory_body()))
        .collect();
    let roles = BTreeMap::from([(id.clone(), role)]);

    assert!(matches!(
        validate_seed_roles(&roles),
        Err(StoryError::TooManyRoleMemories {
            role_id,
            actual,
            maximum: MAX_MEMORIES_PER_ROLE
        }) if role_id == id && actual == MAX_MEMORIES_PER_ROLE + 1
    ));
}

#[test]
fn runtime_memory_count_over_limit_is_rejected() {
    let id = role_id("role");
    let mut role = runtime_role(id.clone());
    role.memories = (0..=MAX_MEMORIES_PER_ROLE)
        .map(|index| {
            let memory_id = memory_id(format!("memory-{index:03}"));
            (
                memory_id.clone(),
                MemoryEntry {
                    id: memory_id,
                    origin: EntryOrigin::Seed,
                    body: memory_body(),
                },
            )
        })
        .collect();
    let roles = BTreeMap::from([(id.clone(), role)]);

    assert!(matches!(
        validate_runtime_roles(&roles),
        Err(StoryError::TooManyRoleMemories {
            role_id,
            actual,
            maximum: MAX_MEMORIES_PER_ROLE
        }) if role_id == id && actual == MAX_MEMORIES_PER_ROLE + 1
    ));
}

#[test]
fn runtime_role_state_at_limit_is_accepted() {
    assert!(validate_runtime_roles(&runtime_roles_with_state_bytes(MAX_ROLE_STATE_BYTES)).is_ok());
}

#[test]
fn runtime_role_state_over_limit_is_rejected() {
    assert!(matches!(
        validate_runtime_roles(&runtime_roles_with_state_bytes(MAX_ROLE_STATE_BYTES + 1)),
        Err(StoryError::RoleStateTooLarge {
            actual,
            maximum: MAX_ROLE_STATE_BYTES
        }) if actual == MAX_ROLE_STATE_BYTES + 1
    ));
}

#[test]
fn counting_writer_stops_at_first_byte_over_limit() {
    let mut writer = CountingWriter::new(4);

    assert_eq!(writer.write(b"1234").expect("bytes at limit should fit"), 4);
    assert!(writer.write(b"5").is_err());
    assert!(writer.exceeded);
    assert_eq!(writer.written, 5);
}

#[test]
fn retrieval_limit_remains_typed() {
    assert!(matches!(
        RetrievalHint::try_new("x".repeat(RetrievalHint::MAX_BYTES + 1)),
        Err(CoreError::RetrievalHintTooLong { .. })
    ));
}
