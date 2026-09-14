use crate::harness::{build_index, entry, fact, index_limits, macros, metadata_of, rule, rule_limits};
use aise::context::activation::index::FrozenPackIndexCache;
use aise::domain::knowledge::activation::{
    ActivationError, ActivationPattern, FrozenPackIndexKey, MATCHER_VERSION, build_frozen_pack_index, macro_digest,
};
use std::sync::Arc;

type LimitMutation = Box<dyn Fn(&mut aise::domain::knowledge::activation::ActivationIndexLimits)>;

fn key(salt: u8) -> FrozenPackIndexKey {
    let mut digest = [0u8; 32];
    digest[31] = salt;
    FrozenPackIndexKey {
        pack_digest: aise::domain::asset::ids::Sha256Digest::from_bytes(digest),
        macro_digest: macro_digest(&macros()),
        matcher_version: MATCHER_VERSION,
    }
}

#[test]
fn frozen_pack_index_is_reused_across_lookups() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let metadata = metadata_of(&entries);
    let cache = FrozenPackIndexCache::new(2);
    let built =
        Arc::new(build_frozen_pack_index(key(1), metadata.values(), &macros(), index_limits(), rule_limits()).unwrap());
    cache.insert(built.clone());
    let fetched = cache.get(&key(1)).unwrap();
    assert!(Arc::ptr_eq(&built, &fetched));
    assert_eq!(cache.len(), 1);
}

#[test]
fn frozen_pack_index_cache_evicts_least_recently_used() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let metadata = metadata_of(&entries);
    let cache = FrozenPackIndexCache::new(1);
    cache.insert(Arc::new(
        build_frozen_pack_index(key(1), metadata.values(), &macros(), index_limits(), rule_limits()).unwrap(),
    ));
    cache.insert(Arc::new(
        build_frozen_pack_index(key(2), metadata.values(), &macros(), index_limits(), rule_limits()).unwrap(),
    ));
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&key(1)).is_none());
    assert!(cache.get(&key(2)).is_some());
}

#[test]
fn index_limits_are_enforced_for_every_field() {
    let entries = vec![
        entry(fact(1), rule("alpha"), "one"),
        entry(fact(2), rule("beta"), "two"),
    ];
    let metadata = metadata_of(&entries);
    let cases: Vec<LimitMutation> = vec![
        Box::new(|limits| limits.max_entries = 1),
        Box::new(|limits| limits.max_literal_patterns = 1),
        Box::new(|limits| limits.max_compiled_bytes = 1),
        Box::new(|limits| limits.max_overlay_entries = 0),
        Box::new(|limits| limits.max_tombstones = 0),
        Box::new(|limits| limits.max_regex_patterns = 0),
        Box::new(|limits| limits.max_regex_program_bytes = 0),
        Box::new(|limits| limits.max_macro_expansion_bytes = 0),
    ];
    for apply in cases {
        let mut limits = index_limits();
        apply(&mut limits);
        let error = build_frozen_pack_index(key(1), metadata.values(), &macros(), limits, rule_limits()).unwrap_err();
        assert!(matches!(error, ActivationError::WorkLimitExceeded { .. }), "{error:?}");
    }
}

#[test]
fn regex_patterns_count_against_the_regex_budget() {
    let entries = vec![
        entry(fact(1), rule("/alpha/i"), "one"),
        entry(fact(2), rule("/beta/i"), "two"),
    ];
    let metadata = metadata_of(&entries);
    let mut limits = index_limits();
    limits.max_regex_patterns = 1;
    let error = build_frozen_pack_index(key(1), metadata.values(), &macros(), limits, rule_limits()).unwrap_err();
    assert!(matches!(
        error,
        ActivationError::WorkLimitExceeded {
            limit: "activation_index_regex_patterns"
        }
    ));
}

#[test]
fn macro_expansion_budget_is_enforced_at_build_time() {
    let mut long_macro = rule("x");
    long_macro.match_rule.keys = vec![ActivationPattern::Literal("{{player_name}}".to_owned())];
    let entries = vec![entry(fact(1), long_macro, "one")];
    let metadata = metadata_of(&entries);
    let mut limits = index_limits();
    limits.max_macro_expansion_bytes = 1;
    let error = build_frozen_pack_index(key(1), metadata.values(), &macros(), limits, rule_limits()).unwrap_err();
    assert!(matches!(
        error,
        ActivationError::WorkLimitExceeded {
            limit: "macro_expansion_bytes"
        }
    ));
}

#[test]
fn snapshot_exposes_pattern_counts_for_runtime_budgets() {
    let entries = vec![
        entry(fact(1), rule("alpha"), "one"),
        entry(fact(2), rule("/beta/i"), "two"),
    ];
    let snapshot = build_index(&entries, index_limits()).unwrap();
    assert_eq!(snapshot.literal_pattern_count(), 1);
    assert_eq!(snapshot.regex_pattern_count(), 1);
}
