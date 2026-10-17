use super::*;
use std::num::NonZeroUsize;

#[test]
fn defaults_match_character_think_contract() {
    let config = ThinkConfig::default();

    assert_eq!(config.max_concurrency, NonZeroUsize::new(4).expect("valid nonzero"));
    assert_eq!(config.max_profile_bytes, 4096);
    assert_eq!(config.max_backstory_bytes, 4096);
    assert_eq!(config.max_memories_in_prompt, 8);
    assert_eq!(config.max_seed_memory_bytes, 8192);
    assert_eq!(config.max_rumor_bytes, 8192);
    assert_eq!(config.max_decision_bytes, 512);
    assert_eq!(config.max_suggested_utterance_bytes, 512);
    assert_eq!(config.max_total_output_bytes, 1024);
}

#[test]
fn zero_concurrency_is_rejected_during_deserialization() {
    let value = toml::toml! {
        max_concurrency = 0
        max_profile_bytes = 4096
        max_backstory_bytes = 4096
        max_memories_in_prompt = 8
        max_seed_memory_bytes = 8192
        max_rumor_bytes = 8192
        max_decision_bytes = 512
        max_suggested_utterance_bytes = 512
        max_total_output_bytes = 1024
    };

    assert!(value.try_into::<ThinkConfig>().is_err());
}

#[test]
fn legacy_max_seed_memories_key_is_rejected() {
    let value = toml::toml! {
        max_concurrency = 4
        max_profile_bytes = 4096
        max_backstory_bytes = 4096
        max_seed_memories = 8
        max_seed_memory_bytes = 8192
        max_rumor_bytes = 8192
        max_decision_bytes = 512
        max_suggested_utterance_bytes = 512
        max_total_output_bytes = 1024
    };

    assert!(value.try_into::<ThinkConfig>().is_err());
}
