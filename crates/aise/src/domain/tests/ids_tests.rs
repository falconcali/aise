use super::{
    CharacterId, DynamicRoleCandidatePool, FactId, MemoryId, RoleId, RoleIdAllocationError, RoleIdHighWater, RumorId,
    allocate_dynamic_role_candidates,
};
use crate::domain::error::{DomainInputError, KnowledgeIdError};

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

#[test]
fn role_id_cannot_collide_with_knowledge_id() {
    for value in ["fact_0001", "rumor_0002", "memory_0003", "fact_10000"] {
        let error = RoleId::try_new(value).expect_err("role id must not collide with a knowledge id shape");
        assert_eq!(error, DomainInputError::RoleIdReservedForKnowledge);
    }
    RoleId::try_new("fact_0001x").expect("non-canonical suffix must remain a valid role id");
    RoleId::try_new("fact_1").expect("unpadded sequence must remain a valid role id");
}

#[test]
fn knowledge_ids_use_canonical_zero_padded_grammar() {
    assert_eq!(FactId::try_new("fact_0001").unwrap().as_str(), "fact_0001");
    assert_eq!(RumorId::try_new("rumor_9999").unwrap().as_str(), "rumor_9999");
    assert_eq!(MemoryId::try_new("memory_10000").unwrap().as_str(), "memory_10000");
}

#[test]
fn knowledge_ids_reject_alternate_spellings() {
    for value in [
        "fact_00001",
        "fact_+001",
        "fact_1",
        "fact_0000",
        "fact_",
        "fact_abcd",
        "rumor_0001x",
    ] {
        FactId::try_new(value)
            .err()
            .or_else(|| RumorId::try_new(value).err())
            .unwrap_or_else(|| {
                panic!("expected {value} to be rejected");
            });
    }
}

#[test]
fn knowledge_id_serializes_as_canonical_string_and_round_trips() {
    let id = RumorId::try_new("rumor_0042").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"rumor_0042\"");
    let round_tripped: RumorId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

#[test]
fn knowledge_id_deserialize_rejects_invalid_grammar() {
    let result: Result<FactId, _> = serde_json::from_str("\"not-a-fact-id\"");
    assert!(result.is_err());
}

#[test]
fn knowledge_id_rejects_sequence_above_sqlite_range() {
    let error = FactId::try_new(format!("fact_{}", i64::MAX as u64 + 1)).expect_err("overflow must be rejected");
    assert_eq!(error, KnowledgeIdError::SequenceOverflow);
}

#[test]
fn dynamic_role_id_shape_matches_the_canonical_grammar() {
    for value in ["role_0001", "role_0002", "role_10000"] {
        assert!(
            RoleId::is_reserved_dynamic_shape(value),
            "{value} should match the dynamic shape"
        );
    }
    for value in ["role_1", "role_00001", "role_", "protagonist", "npc_merchant"] {
        assert!(
            !RoleId::is_reserved_dynamic_shape(value),
            "{value} should not match the dynamic shape"
        );
    }
}

#[test]
fn role_id_accepts_dynamic_shaped_values_for_allocator_use() {
    RoleId::try_new("role_0001").expect("dynamic-shaped role id must remain constructible for the allocator");
}

#[test]
fn allocate_dynamic_role_candidates_renders_sequential_prefix_from_zero() {
    let pool = allocate_dynamic_role_candidates(RoleIdHighWater::zero(), 3).expect("allocation succeeds");
    let rendered: Vec<&str> = pool.candidates.iter().map(RoleId::as_str).collect();
    assert_eq!(rendered, vec!["role_0001", "role_0002", "role_0003"]);
    assert_eq!(pool.base_high_water, RoleIdHighWater::zero());
}

#[test]
fn allocate_dynamic_role_candidates_continues_from_a_non_zero_base() {
    let pool = allocate_dynamic_role_candidates(RoleIdHighWater::new(5), 2).expect("allocation succeeds");
    let rendered: Vec<&str> = pool.candidates.iter().map(RoleId::as_str).collect();
    assert_eq!(rendered, vec!["role_0006", "role_0007"]);
}

#[test]
fn allocate_dynamic_role_candidates_rejects_overflow() {
    let error =
        allocate_dynamic_role_candidates(RoleIdHighWater::new(u64::MAX), 1).expect_err("overflow must be rejected");
    assert_eq!(error, RoleIdAllocationError::AllocationOverflow);
}

#[test]
fn dynamic_role_candidate_pool_reports_position_of_a_candidate() {
    let pool = allocate_dynamic_role_candidates(RoleIdHighWater::zero(), 2).unwrap();
    let first = RoleId::try_new("role_0001").unwrap();
    let unknown = RoleId::try_new("role_0099").unwrap();
    assert_eq!(pool.position_of(&first), Some(0));
    assert_eq!(pool.position_of(&unknown), None);
    assert!(!pool.is_empty());
}

#[test]
fn empty_dynamic_role_candidate_pool_reports_empty() {
    let pool: DynamicRoleCandidatePool = allocate_dynamic_role_candidates(RoleIdHighWater::zero(), 0).unwrap();
    assert!(pool.is_empty());
}
