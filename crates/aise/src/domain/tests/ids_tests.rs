use super::{CharacterId, RoleId};
use crate::domain::error::DomainInputError;

#[test]
fn character_id_accepts_canonical_lowercase_hyphenated_uuid() {
    let value = "550e8400-e29b-41d4-a716-446655440000";
    let id = CharacterId::try_new(value).expect("canonical uuid should be accepted");
    assert_eq!(id.as_str(), value);
}

#[test]
fn character_id_normalizes_uppercase_uuid() {
    let id = CharacterId::try_new("550E8400-E29B-41D4-A716-446655440000").expect("uppercase uuid should be accepted");
    assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn character_id_rejects_nil_uuid() {
    let error = CharacterId::try_new("00000000-0000-0000-0000-000000000000").expect_err("nil uuid must be rejected");
    assert_eq!(error, DomainInputError::InvalidCharacterId);
}

#[test]
fn character_id_rejects_malformed_uuid() {
    let error = CharacterId::try_new("not-a-uuid").expect_err("malformed uuid must be rejected");
    assert_eq!(error, DomainInputError::InvalidCharacterId);
}

#[test]
fn character_id_new_uuid_is_non_nil_and_round_trips() {
    let id = CharacterId::new_uuid();
    let round_tripped = CharacterId::try_new(id.as_str()).expect("generated id should round-trip");
    assert_eq!(id, round_tripped);
}

#[test]
fn character_id_serializes_as_canonical_string() {
    let id = CharacterId::try_new("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"550e8400-e29b-41d4-a716-446655440000\"");
    let round_tripped: CharacterId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

#[test]
fn character_id_deserialize_rejects_invalid_value() {
    let result: Result<CharacterId, _> = serde_json::from_str("\"not-a-uuid\"");
    assert!(result.is_err());
}

#[test]
fn role_id_accepts_dots_underscores_and_hyphens() {
    for value in ["protagonist", "npc.merchant", "npc_merchant", "npc-merchant-2"] {
        RoleId::try_new(value).unwrap_or_else(|_| panic!("expected {value} to be accepted"));
    }
}

#[test]
fn role_id_rejects_empty() {
    let error = RoleId::try_new("").expect_err("empty role id must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_uppercase() {
    let error = RoleId::try_new("Protagonist").expect_err("uppercase role id must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_whitespace() {
    let error = RoleId::try_new("npc merchant").expect_err("whitespace role id must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_leading_separator() {
    let error = RoleId::try_new(".npc").expect_err("leading separator must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_trailing_separator() {
    let error = RoleId::try_new("npc.").expect_err("trailing separator must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_repeated_separator() {
    let error = RoleId::try_new("npc..merchant").expect_err("repeated separator must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_control_character() {
    let error = RoleId::try_new("npc\tmerchant").expect_err("control character must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_rejects_over_max_bytes() {
    let value = "a".repeat(RoleId::MAX_BYTES + 1);
    let error = RoleId::try_new(value).expect_err("over-length role id must be rejected");
    assert_eq!(error, DomainInputError::InvalidRoleId);
}

#[test]
fn role_id_accepts_exact_max_bytes() {
    let value = "a".repeat(RoleId::MAX_BYTES);
    RoleId::try_new(value).expect("exact max-length role id should be accepted");
}

#[test]
fn role_id_serializes_and_round_trips_through_json() {
    let id = RoleId::try_new("protagonist").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"protagonist\"");
    let round_tripped: RoleId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

#[test]
fn character_id_and_role_id_with_duplicate_display_names_remain_distinct() {
    let first_character = CharacterId::try_new("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let second_character = CharacterId::try_new("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
    assert_ne!(first_character, second_character);

    let first_role = RoleId::try_new("hero").unwrap();
    let second_role = RoleId::try_new("hero-2").unwrap();
    assert_ne!(first_role, second_role);
}

#[test]
fn character_id_and_role_id_ordering_is_deterministic() {
    let mut roles = [RoleId::try_new("zeta").unwrap(), RoleId::try_new("alpha").unwrap()];
    roles.sort();
    assert_eq!(roles[0].as_str(), "alpha");
    assert_eq!(roles[1].as_str(), "zeta");
}
