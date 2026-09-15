use super::*;

#[test]
fn frozen_cache_starts_empty() {
    let cache = FrozenPackIndexCache::new(1, 1);
    assert!(cache.is_empty());
}
