use super::*;

#[test]
fn fragment_cache_stats_start_at_zero() {
    let cache = LruFragmentMatchCache::new(crate::config::ActivationConfig::default().cache);
    assert_eq!(cache.stats(), FragmentMatchCacheStats::default());
}
