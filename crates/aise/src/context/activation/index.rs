use crate::config::FragmentMatchCacheLimits;
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::ids::StoryId;
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::knowledge::activation::{
    ActivationEntryMetadata, ActivationError, ActivationIndexLimits, ActivationIndexSnapshot,
    ActivationIndexSnapshotRef, ActivationMacroValues, ActivationRuleLimits, FragmentPatternMatch, FrozenLiteralIndex,
    FrozenPackIndex, FrozenPackIndexKey, FrozenRegexSet, ScanFragment, ScanFragmentId, ScanFragmentKind,
    build_frozen_pack_index,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

pub use crate::domain::knowledge::activation::index::{MATCHER_VERSION, macro_digest};

#[derive(Debug)]
pub struct ActivationOverlayIndex {
    pub overlay_version: u64,
    pub literal_index: Arc<FrozenLiteralIndex>,
    pub regex_set: Arc<FrozenRegexSet>,
}

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
    pub matches: Arc<Vec<FragmentPatternMatch>>,
    pub estimated_bytes: usize,
}

pub trait FragmentMatchCache: Send + Sync {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<FragmentMatchCacheValue>;
    fn insert(&self, key: FragmentMatchCacheKey, value: FragmentMatchCacheValue);
    fn invalidate_story(&self, story_id: &StoryId);
}

struct FragmentCacheState {
    entries: HashMap<FragmentMatchCacheKey, (u64, FragmentMatchCacheValue)>,
    order: BTreeMap<u64, FragmentMatchCacheKey>,
    per_story: HashMap<StoryId, usize>,
    total_bytes: usize,
    tick: u64,
}

pub struct InMemoryFragmentMatchCache {
    limits: FragmentMatchCacheLimits,
    state: Mutex<FragmentCacheState>,
}

impl InMemoryFragmentMatchCache {
    pub fn new(limits: FragmentMatchCacheLimits) -> Self {
        Self {
            limits,
            state: Mutex::new(FragmentCacheState {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                per_story: HashMap::new(),
                total_bytes: 0,
                tick: 0,
            }),
        }
    }

    pub fn limits(&self) -> FragmentMatchCacheLimits {
        self.limits
    }

    pub fn admits(&self, value: &FragmentMatchCacheValue) -> bool {
        value.matches.len() <= self.limits.max_matches_per_fragment
            && value.estimated_bytes <= self.limits.max_evidence_bytes_per_fragment
    }
}

impl FragmentMatchCache for InMemoryFragmentMatchCache {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<FragmentMatchCacheValue> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        let previous = {
            let (slot, value) = state.entries.get_mut(key)?;
            let previous = *slot;
            *slot = tick;
            let value = value.clone();
            state.order.remove(&previous);
            state.order.insert(tick, key.clone());
            value
        };
        Some(previous)
    }

    fn insert(&self, key: FragmentMatchCacheKey, value: FragmentMatchCacheValue) {
        if !self.admits(&value) {
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        if let Some((previous, old)) = state.entries.remove(&key) {
            state.order.remove(&previous);
            state.total_bytes = state.total_bytes.saturating_sub(old.estimated_bytes);
            decrement_story(&mut state.per_story, &key.story_id);
        }
        state.total_bytes = state.total_bytes.saturating_add(value.estimated_bytes);
        *state.per_story.entry(key.story_id.clone()).or_default() += 1;
        state.order.insert(tick, key.clone());
        state.entries.insert(key.clone(), (tick, value));
        enforce_story_limit(&mut state, &key.story_id, self.limits.max_fragments_per_story);
        enforce_global_limits(
            &mut state,
            self.limits.max_cached_stories,
            self.limits.max_total_estimated_bytes,
        );
    }

    fn invalidate_story(&self, story_id: &StoryId) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let doomed = state
            .entries
            .keys()
            .filter(|key| &key.story_id == story_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in doomed {
            remove_key(&mut state, &key);
        }
    }
}

fn decrement_story(per_story: &mut HashMap<StoryId, usize>, story_id: &StoryId) {
    let Some(count) = per_story.get_mut(story_id) else {
        return;
    };
    *count = count.saturating_sub(1);
    if *count == 0 {
        per_story.remove(story_id);
    }
}

fn remove_key(state: &mut FragmentCacheState, key: &FragmentMatchCacheKey) {
    let Some((tick, value)) = state.entries.remove(key) else {
        return;
    };
    state.order.remove(&tick);
    state.total_bytes = state.total_bytes.saturating_sub(value.estimated_bytes);
    decrement_story(&mut state.per_story, &key.story_id);
}

fn enforce_story_limit(state: &mut FragmentCacheState, story_id: &StoryId, max_fragments: usize) {
    while state.per_story.get(story_id).copied().unwrap_or_default() > max_fragments {
        let Some(key) = state
            .order
            .iter()
            .find(|(_, key)| &key.story_id == story_id)
            .map(|(_, key)| key.clone())
        else {
            return;
        };
        remove_key(state, &key);
    }
}

fn enforce_global_limits(state: &mut FragmentCacheState, max_stories: usize, max_bytes: usize) {
    while state.per_story.len() > max_stories || state.total_bytes > max_bytes {
        let Some(key) = state.order.values().next().cloned() else {
            return;
        };
        remove_key(state, &key);
    }
}

struct PackIndexCacheState {
    entries: HashMap<FrozenPackIndexKey, (u64, Arc<FrozenPackIndex>)>,
    order: BTreeMap<u64, FrozenPackIndexKey>,
    tick: u64,
}

pub struct FrozenPackIndexCache {
    capacity: usize,
    state: Mutex<PackIndexCacheState>,
}

impl FrozenPackIndexCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(PackIndexCacheState {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                tick: 0,
            }),
        }
    }

    pub fn get(&self, key: &FrozenPackIndexKey) -> Option<Arc<FrozenPackIndex>> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        let (previous, index) = {
            let (slot, index) = state.entries.get_mut(key)?;
            let previous = *slot;
            *slot = tick;
            (previous, index.clone())
        };
        state.order.remove(&previous);
        state.order.insert(tick, key.clone());
        Some(index)
    }

    pub fn insert(&self, index: Arc<FrozenPackIndex>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let key = index.key.clone();
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        if let Some((previous, _)) = state.entries.remove(&key) {
            state.order.remove(&previous);
        }
        state.order.insert(tick, key.clone());
        state.entries.insert(key, (tick, index));
        while state.entries.len() > self.capacity {
            let Some(oldest) = state.order.values().next().cloned() else {
                break;
            };
            if let Some((previous, _)) = state.entries.remove(&oldest) {
                state.order.remove(&previous);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.state.lock().map(|state| state.entries.len()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn build_overlay_index(
    overlay_version: u64,
    entries: &BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
    rule_limits: ActivationRuleLimits,
) -> Result<ActivationOverlayIndex, ActivationError> {
    let overlay_limits = ActivationIndexLimits {
        max_entries: limits.max_overlay_entries,
        ..limits
    };
    let key = FrozenPackIndexKey {
        pack_digest: Sha256Digest::from_bytes([0u8; 32]),
        macro_digest: macro_digest(macros),
        matcher_version: MATCHER_VERSION,
    };
    let index = build_frozen_pack_index(
        key,
        entries.values().filter(|entry| !entry.from_pack),
        macros,
        overlay_limits,
        rule_limits,
    )?;
    Ok(ActivationOverlayIndex {
        overlay_version,
        literal_index: index.literal_index,
        regex_set: index.regex_set,
    })
}

pub fn compose_index_snapshot(
    reference: ActivationIndexSnapshotRef,
    metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
    pack_index: &FrozenPackIndex,
    overlay: &ActivationOverlayIndex,
) -> ActivationIndexSnapshot {
    ActivationIndexSnapshot::new(
        reference,
        metadata,
        pack_index.literal_index.clone(),
        pack_index.regex_set.clone(),
        overlay.literal_index.clone(),
        overlay.regex_set.clone(),
    )
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
