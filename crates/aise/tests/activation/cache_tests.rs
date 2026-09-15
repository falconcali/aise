use crate::harness::{
    ExecuteSpec, build_index, entry, execute, fact, fragment, ids, index_limits, macros, match_all, match_all_cached,
    new_cache, rule,
};
use aise::config::ActivationConfig;
use aise::context::activation::fragment_cache::{
    FragmentMatchCache, FragmentMatchCacheValue, LruFragmentMatchCache, fragment_cache_key,
};
use aise::domain::knowledge::activation::{ActivationScanBuffer, ScanFragmentKind, macro_digest};
use std::sync::Arc;

#[test]
fn cached_and_uncached_runs_produce_identical_results() {
    let entries = vec![
        entry(fact(1), rule("alpha"), "one"),
        entry(fact(2), rule("beta"), "two"),
    ];
    let fragments = vec![
        fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha"),
        fragment(ScanFragmentKind::NarrativeEvent, 1, 1, "beta"),
    ];
    let cold = execute(ExecuteSpec::new(entries.clone(), fragments.clone(), 4)).unwrap();
    let cache = Arc::new(new_cache());
    let warm_first =
        execute(ExecuteSpec::new(entries.clone(), fragments.clone(), 4).with_cache(cache.clone())).unwrap();
    let warm_second = execute(ExecuteSpec::new(entries, fragments, 4).with_cache(cache)).unwrap();
    assert_eq!(ids(&cold.activated), ids(&warm_first.activated));
    assert_eq!(ids(&cold.activated), ids(&warm_second.activated));
    assert_eq!(
        serde_json::to_value(&cold.activated).unwrap(),
        serde_json::to_value(&warm_second.activated).unwrap()
    );
}

#[test]
fn cache_returns_the_same_matches_as_direct_matching() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let index = build_index(&entries, index_limits()).unwrap();
    let buffer =
        ActivationScanBuffer::try_new(vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")], 16, 4096)
            .unwrap();
    let direct = match_all(&index, &buffer);
    let cache = new_cache();
    let first = match_all_cached(&index, &buffer, &cache);
    let second = match_all_cached(&index, &buffer, &cache);
    for item in buffer.fragments() {
        let expected = direct.get(&item.id).unwrap();
        assert_eq!(expected.as_slice(), first.get(&item.id).unwrap().as_slice());
        assert_eq!(expected.as_slice(), second.get(&item.id).unwrap().as_slice());
    }
}

#[test]
fn cache_overlay_version_change_misses() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let index = build_index(&entries, index_limits()).unwrap();
    let buffer =
        ActivationScanBuffer::try_new(vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")], 16, 4096)
            .unwrap();
    let cache = new_cache();
    match_all_cached(&index, &buffer, &cache);
    let digest = macro_digest(&macros());
    let mut key = fragment_cache_key(&crate::harness::story_id(), &index.reference, &digest, &buffer.fragments()[0]);
    assert!(cache.get(&key).is_some());
    key.overlay_version = key.overlay_version.saturating_add(1);
    assert!(cache.get(&key).is_none());
}

#[test]
fn oversized_fragment_values_are_not_cached() {
    let entries = vec![entry(fact(1), rule("alpha"), "one")];
    let index = build_index(&entries, index_limits()).unwrap();
    let buffer =
        ActivationScanBuffer::try_new(vec![fragment(ScanFragmentKind::PlayerContribution, 1, 0, "alpha")], 16, 4096)
            .unwrap();
    let mut limits = ActivationConfig::default().cache;
    limits.max_matches_per_fragment = 1;
    let cache = LruFragmentMatchCache::new(limits);
    let digest = macro_digest(&macros());
    let fragment_ref = &buffer.fragments()[0];
    let key = fragment_cache_key(&crate::harness::story_id(), &index.reference, &digest, fragment_ref);
    let matches = index.match_fragment(fragment_ref);
    cache
        .insert(
            key.clone(),
            FragmentMatchCacheValue {
                matches: vec![matches[0].clone(), matches[0].clone()],
            },
        )
        .unwrap();
    assert!(cache.get(&key).is_none());
    cache.insert(key.clone(), FragmentMatchCacheValue { matches }).unwrap();
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.stats().insert_rejections, 1);
}
