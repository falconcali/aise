use super::*;

#[test]
fn defaults_match_generate_contract() {
    let config = GenerateConfig::default();

    assert_eq!(config.max_role_bytes_per_role, 16_384);
    assert_eq!(config.max_role_context_bytes, 393_216);
    assert_eq!(config.max_seed_memories_per_role, 4);
    assert_eq!(config.max_decision_context_bytes, 16_384);
}
