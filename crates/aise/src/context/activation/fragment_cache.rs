use crate::config::FragmentMatchCacheLimits;
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::ids::StoryId;
use crate::domain::knowledge::activation::{
    ActivationError, ActivationIndexSnapshotRef, ScanFragment, ScanFragmentId, ScanFragmentKind,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

pub use crate::domain::knowledge::activation::FragmentPatternMatch;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentMatchCacheKey {
    pub story_id: StoryId,
    pub fragment_id: ScanFragmentId,
    pub fragment_kind: ScanFragmentKind,
    pub content_hash: Sha256Digest,
    pub pack_digest: Sha256Digest,
    pub overlay_version: u64,
    pub matcher_version: u32,
    pub macro_digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct FragmentMatchCacheValue {
    pub matches: Vec<FragmentPatternMatch>,
}

pub trait FragmentMatchCache: Send + Sync {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<Arc<FragmentMatchCacheValue>>;
    fn insert(&self, key: FragmentMatchCacheKey, value: FragmentMatchCacheValue) -> Result<(), ActivationError>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FragmentMatchCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insert_rejections: u64,
    pub evictions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CachePartitionKey(StoryId);

struct FragmentMatchCacheEntry {
    tick: u64,
    value: Arc<FragmentMatchCacheValue>,
    estimated_bytes: usize,
}

struct LruFragmentMatchState {
    entries: HashMap<FragmentMatchCacheKey, FragmentMatchCacheEntry>,
    order: BTreeMap<u64, FragmentMatchCacheKey>,
    per_partition: HashMap<CachePartitionKey, usize>,
    total_estimated_bytes: usize,
    tick: u64,
    stats: FragmentMatchCacheStats,
}

pub struct LruFragmentMatchCache {
    inner: Mutex<LruFragmentMatchState>,
    limits: FragmentMatchCacheLimits,
}

impl LruFragmentMatchCache {
    pub fn new(limits: FragmentMatchCacheLimits) -> Self {
        Self {
            inner: Mutex::new(LruFragmentMatchState {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                per_partition: HashMap::new(),
                total_estimated_bytes: 0,
                tick: 0,
                stats: FragmentMatchCacheStats::default(),
            }),
            limits,
        }
    }

    pub fn stats(&self) -> FragmentMatchCacheStats {
        self.inner.lock().unwrap_or_else(|error| error.into_inner()).stats
    }

    pub fn limits(&self) -> FragmentMatchCacheLimits {
        self.limits
    }
}

impl FragmentMatchCache for LruFragmentMatchCache {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<Arc<FragmentMatchCacheValue>> {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        let Some(entry) = state.entries.get_mut(key) else {
            state.stats.misses = state.stats.misses.saturating_add(1);
            return None;
        };
        let previous = entry.tick;
        entry.tick = tick;
        let value = entry.value.clone();
        state.order.remove(&previous);
        state.order.insert(tick, key.clone());
        state.stats.hits = state.stats.hits.saturating_add(1);
        Some(value)
    }

    fn insert(&self, key: FragmentMatchCacheKey, value: FragmentMatchCacheValue) -> Result<(), ActivationError> {
        let estimated_bytes = estimate_match_bytes(&value.matches);
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if value.matches.len() > self.limits.max_matches_per_fragment
            || estimated_bytes > self.limits.max_evidence_bytes_per_fragment
            || estimated_bytes > self.limits.max_total_estimated_bytes
        {
            state.stats.insert_rejections = state.stats.insert_rejections.saturating_add(1);
            return Ok(());
        }
        remove_key(&mut state, &key, false);
        let partition = partition_key(&key);
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        state.total_estimated_bytes = state.total_estimated_bytes.saturating_add(estimated_bytes);
        *state.per_partition.entry(partition.clone()).or_default() += 1;
        state.order.insert(tick, key.clone());
        state.entries.insert(
            key,
            FragmentMatchCacheEntry {
                tick,
                value: Arc::new(value),
                estimated_bytes,
            },
        );
        enforce_partition_limit(&mut state, &partition, self.limits.max_fragments_per_story);
        enforce_global_limits(
            &mut state,
            self.limits.max_cached_stories,
            self.limits.max_total_estimated_bytes,
        );
        Ok(())
    }
}

fn partition_key(key: &FragmentMatchCacheKey) -> CachePartitionKey {
    CachePartitionKey(key.story_id.clone())
}

fn decrement_partition(per_partition: &mut HashMap<CachePartitionKey, usize>, partition: &CachePartitionKey) {
    let Some(count) = per_partition.get_mut(partition) else {
        return;
    };
    *count = count.saturating_sub(1);
    if *count == 0 {
        per_partition.remove(partition);
    }
}

fn remove_key(state: &mut LruFragmentMatchState, key: &FragmentMatchCacheKey, eviction: bool) {
    let Some(entry) = state.entries.remove(key) else {
        return;
    };
    state.order.remove(&entry.tick);
    state.total_estimated_bytes = state.total_estimated_bytes.saturating_sub(entry.estimated_bytes);
    decrement_partition(&mut state.per_partition, &partition_key(key));
    if eviction {
        state.stats.evictions = state.stats.evictions.saturating_add(1);
    }
}

fn enforce_partition_limit(state: &mut LruFragmentMatchState, partition: &CachePartitionKey, max_fragments: usize) {
    while state.per_partition.get(partition).copied().unwrap_or_default() > max_fragments {
        let Some(key) = state.order.values().find(|key| partition_key(key) == *partition).cloned() else {
            return;
        };
        remove_key(state, &key, true);
    }
}

fn enforce_global_limits(state: &mut LruFragmentMatchState, max_partitions: usize, max_bytes: usize) {
    while state.per_partition.len() > max_partitions || state.total_estimated_bytes > max_bytes {
        let Some(key) = state.order.values().next().cloned() else {
            return;
        };
        remove_key(state, &key, true);
    }
}

pub fn estimate_match_bytes(matches: &[FragmentPatternMatch]) -> usize {
    matches.len().saturating_mul(std::mem::size_of::<FragmentPatternMatch>())
}

pub fn fragment_cache_key(
    story_id: &StoryId,
    reference: &ActivationIndexSnapshotRef,
    macro_digest: &Sha256Digest,
    fragment: &ScanFragment,
) -> FragmentMatchCacheKey {
    FragmentMatchCacheKey {
        story_id: story_id.clone(),
        fragment_id: fragment.id.clone(),
        fragment_kind: fragment.kind,
        content_hash: fragment.content_hash.clone(),
        pack_digest: reference.pack_digest.clone(),
        overlay_version: reference.overlay_version,
        matcher_version: reference.matcher_version,
        macro_digest: macro_digest.clone(),
    }
}

#[cfg(test)]
#[path = "tests/fragment_cache_tests.rs"]
mod tests;
