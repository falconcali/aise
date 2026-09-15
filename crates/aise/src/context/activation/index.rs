use crate::domain::asset::ids::Sha256Digest;
use crate::domain::knowledge::KnowledgeSourceId;
use crate::domain::knowledge::activation::{
    ActivationEntryMetadata, ActivationError, ActivationIndexLimits, ActivationIndexSnapshot,
    ActivationIndexSnapshotRef, ActivationMacroValues, ActivationRuleLimits, build_frozen_pack_index,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

pub use crate::domain::knowledge::activation::index::{
    CompiledPatternRef, FrozenLiteralIndex, FrozenPackIndex, FrozenPackIndexKey, FrozenRegexSet, MATCHER_VERSION,
    macro_digest,
};

#[derive(Debug)]
pub struct ActivationOverlayIndex {
    pub overlay_version: u64,
    pub upserts: Vec<ActivationEntryMetadata>,
    pub tombstones: BTreeSet<KnowledgeSourceId>,
    pub literal_index: Arc<FrozenLiteralIndex>,
    pub regex_set: Arc<FrozenRegexSet>,
}

struct PackIndexCacheState {
    entries: HashMap<FrozenPackIndexKey, (u64, Arc<FrozenPackIndex>)>,
    order: BTreeMap<u64, FrozenPackIndexKey>,
    total_estimated_bytes: usize,
    tick: u64,
}

pub struct FrozenPackIndexCache {
    max_packs: usize,
    max_total_estimated_bytes: usize,
    state: Mutex<PackIndexCacheState>,
}

impl FrozenPackIndexCache {
    pub fn new(max_packs: usize, max_total_estimated_bytes: usize) -> Self {
        Self {
            max_packs: max_packs.max(1),
            max_total_estimated_bytes: max_total_estimated_bytes.max(1),
            state: Mutex::new(PackIndexCacheState {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                total_estimated_bytes: 0,
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

    pub fn insert(&self, index: Arc<FrozenPackIndex>) -> Result<(), ActivationError> {
        let estimated_bytes = index.estimated_bytes();
        if estimated_bytes > self.max_total_estimated_bytes {
            return Err(ActivationError::WorkLimitExceeded {
                limit: "max_compiled_bytes",
            });
        }
        let mut state = self.state.lock().map_err(|_| ActivationError::WorkLimitExceeded {
            limit: "max_compiled_bytes",
        })?;
        let key = index.key.clone();
        let tick = state.tick.saturating_add(1);
        state.tick = tick;
        if let Some((previous, replaced)) = state.entries.remove(&key) {
            state.order.remove(&previous);
            state.total_estimated_bytes = state.total_estimated_bytes.saturating_sub(replaced.estimated_bytes());
        }
        state.total_estimated_bytes = state.total_estimated_bytes.saturating_add(estimated_bytes);
        state.order.insert(tick, key.clone());
        state.entries.insert(key, (tick, index));
        while state.entries.len() > self.max_packs || state.total_estimated_bytes > self.max_total_estimated_bytes {
            let Some(oldest) = state.order.values().next().cloned() else {
                break;
            };
            if let Some((previous, removed)) = state.entries.remove(&oldest) {
                state.order.remove(&previous);
                state.total_estimated_bytes = state.total_estimated_bytes.saturating_sub(removed.estimated_bytes());
            }
        }
        Ok(())
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
    pack_index: &FrozenPackIndex,
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
    rule_limits: ActivationRuleLimits,
) -> Result<ActivationOverlayIndex, ActivationError> {
    let upserts = entries.values().filter(|entry| !entry.from_pack).cloned().collect::<Vec<_>>();
    if upserts.len() > limits.max_overlay_entries {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "max_overlay_entries",
        });
    }
    let tombstones = pack_index
        .metadata
        .keys()
        .filter(|source_id| !entries.contains_key(*source_id))
        .cloned()
        .collect::<BTreeSet<_>>();
    if tombstones.len() > limits.max_tombstones {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "max_tombstones",
        });
    }
    let overlay_limits = ActivationIndexLimits {
        max_entries: limits.max_overlay_entries,
        ..limits
    };
    let key = FrozenPackIndexKey {
        pack_digest: Sha256Digest::from_bytes([0u8; 32]),
        macro_digest: macro_digest(macros),
        matcher_version: MATCHER_VERSION,
    };
    let index = build_frozen_pack_index(key, upserts.iter(), macros, overlay_limits, rule_limits)?;
    Ok(ActivationOverlayIndex {
        overlay_version,
        upserts,
        tombstones,
        literal_index: index.literal_index,
        regex_set: index.regex_set,
    })
}

pub fn compose_index_snapshot(
    reference: ActivationIndexSnapshotRef,
    pack_index: &FrozenPackIndex,
    overlay: &ActivationOverlayIndex,
    limits: ActivationIndexLimits,
) -> Result<ActivationIndexSnapshot, ActivationError> {
    let literal_patterns = pack_index.literal_index.len().saturating_add(overlay.literal_index.len());
    if literal_patterns > limits.max_literal_patterns {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "max_literal_patterns",
        });
    }
    let regex_patterns = pack_index.regex_set.len().saturating_add(overlay.regex_set.len());
    if regex_patterns > limits.max_regex_patterns {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "max_regex_patterns",
        });
    }
    let compiled_bytes = pack_index
        .estimated_bytes()
        .saturating_add(overlay.literal_index.compiled_bytes())
        .saturating_add(overlay.regex_set.compiled_bytes());
    if compiled_bytes > limits.max_compiled_bytes {
        return Err(ActivationError::WorkLimitExceeded {
            limit: "max_compiled_bytes",
        });
    }
    let mut metadata = pack_index.metadata.clone();
    for source_id in &overlay.tombstones {
        metadata.remove(source_id);
    }
    for entry in &overlay.upserts {
        metadata.insert(entry.source_id.clone(), entry.clone());
    }
    if metadata.len() > limits.max_entries {
        return Err(ActivationError::WorkLimitExceeded { limit: "max_entries" });
    }
    Ok(ActivationIndexSnapshot::new(
        reference,
        metadata,
        pack_index.literal_index.clone(),
        pack_index.regex_set.clone(),
        overlay.literal_index.clone(),
        overlay.regex_set.clone(),
    ))
}

#[cfg(test)]
#[path = "tests/index_tests.rs"]
mod tests;
