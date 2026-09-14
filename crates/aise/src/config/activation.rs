use super::error::ConfigError;
use crate::domain::knowledge::activation::{ActivationGroupKey, ActivationRuleLimits};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationConfig {
    pub rule: ActivationRuleLimitsConfig,
    pub index: ActivationIndexLimits,
    pub runtime: ActivationRuntimeLimits,
    pub cache: FragmentMatchCacheLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRuleLimitsConfig {
    pub max_primary_patterns_per_entry: usize,
    pub max_secondary_patterns_per_entry: usize,
    pub max_pattern_bytes: usize,
    pub max_regex_program_bytes: usize,
    pub max_groups_per_entry: usize,
    pub max_group_key_bytes: usize,
    pub max_macro_value_bytes: usize,
    pub max_macro_expansion_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationIndexLimits {
    pub max_entries: usize,
    pub max_overlay_entries: usize,
    pub max_tombstones: usize,
    pub max_literal_patterns: usize,
    pub max_regex_patterns: usize,
    pub max_compiled_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentMatchCacheLimits {
    pub max_cached_stories: usize,
    pub max_fragments_per_story: usize,
    pub max_matches_per_fragment: usize,
    pub max_evidence_bytes_per_fragment: usize,
    pub max_total_estimated_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRuntimeLimits {
    pub minimum_activations: usize,
    pub initial_scan_depth: u16,
    pub max_scan_depth: u16,
    pub include_summary_at_max_depth: bool,
    pub max_scan_fragments: usize,
    pub max_scan_bytes: usize,
    pub max_scan_tokens: u64,
    pub max_literal_patterns: usize,
    pub max_regex_patterns: usize,
    pub max_pattern_matches: usize,
    pub max_candidates_per_round: usize,
    pub max_recursion_steps: u16,
    pub max_recursion_fragments: usize,
    pub max_recursion_bytes: usize,
    pub max_recursion_tokens: u64,
    pub max_activated_entries: usize,
    pub max_depth_expansions: u16,
    pub max_external_candidates: usize,
    pub max_evidence_per_entry: usize,
    pub max_evidence_bytes: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_single_entry_bytes: usize,
    pub reserved_tokens: u64,
    pub mandatory_tokens: u64,
}

impl Default for ActivationConfig {
    fn default() -> Self {
        Self {
            rule: ActivationRuleLimitsConfig {
                max_primary_patterns_per_entry: 32,
                max_secondary_patterns_per_entry: 32,
                max_pattern_bytes: 1024,
                max_regex_program_bytes: 64 * 1024,
                max_groups_per_entry: 8,
                max_group_key_bytes: ActivationGroupKey::MAX_BYTES,
                max_macro_value_bytes: 1024,
                max_macro_expansion_bytes: 4096,
            },
            index: ActivationIndexLimits {
                max_entries: 2048,
                max_overlay_entries: 512,
                max_tombstones: 512,
                max_literal_patterns: 8192,
                max_regex_patterns: 1024,
                max_compiled_bytes: 16 * 1024 * 1024,
            },
            runtime: ActivationRuntimeLimits {
                minimum_activations: 0,
                initial_scan_depth: 1,
                max_scan_depth: 8,
                include_summary_at_max_depth: true,
                max_scan_fragments: 64,
                max_scan_bytes: 256 * 1024,
                max_scan_tokens: 64 * 1024,
                max_literal_patterns: 8192,
                max_regex_patterns: 1024,
                max_pattern_matches: 16384,
                max_candidates_per_round: 2048,
                max_recursion_steps: 8,
                max_recursion_fragments: 64,
                max_recursion_bytes: 128 * 1024,
                max_recursion_tokens: 32 * 1024,
                max_activated_entries: 256,
                max_depth_expansions: 8,
                max_external_candidates: 256,
                max_evidence_per_entry: 32,
                max_evidence_bytes: 256 * 1024,
                max_items_per_audience: 128,
                max_tokens_per_audience: 16 * 1024,
                max_total_items: 256,
                max_total_tokens: 32 * 1024,
                max_single_entry_bytes: 32 * 1024,
                reserved_tokens: 4096,
                mandatory_tokens: 4096,
            },
            cache: FragmentMatchCacheLimits {
                max_cached_stories: 64,
                max_fragments_per_story: 128,
                max_matches_per_fragment: 1024,
                max_evidence_bytes_per_fragment: 16 * 1024,
                max_total_estimated_bytes: 64 * 1024 * 1024,
            },
        }
    }
}

impl ActivationConfig {
    pub fn domain_runtime_limits(&self) -> crate::domain::knowledge::activation::ActivationRuntimeLimits {
        let source = self.runtime;
        crate::domain::knowledge::activation::ActivationRuntimeLimits {
            minimum_activations: source.minimum_activations,
            initial_scan_depth: source.initial_scan_depth,
            max_scan_depth: source.max_scan_depth,
            include_summary_at_max_depth: source.include_summary_at_max_depth,
            max_scan_fragments: source.max_scan_fragments,
            max_scan_bytes: source.max_scan_bytes,
            max_scan_tokens: source.max_scan_tokens,
            max_literal_patterns: source.max_literal_patterns,
            max_regex_patterns: source.max_regex_patterns,
            max_pattern_matches: source.max_pattern_matches,
            max_candidates_per_round: source.max_candidates_per_round,
            max_recursion_steps: source.max_recursion_steps,
            max_recursion_fragments: source.max_recursion_fragments,
            max_recursion_bytes: source.max_recursion_bytes,
            max_recursion_tokens: source.max_recursion_tokens,
            max_activated_entries: source.max_activated_entries,
            max_depth_expansions: source.max_depth_expansions,
            max_external_candidates: source.max_external_candidates,
            max_evidence_per_entry: source.max_evidence_per_entry,
            max_evidence_bytes: source.max_evidence_bytes,
            max_items_per_audience: source.max_items_per_audience,
            max_tokens_per_audience: source.max_tokens_per_audience,
            max_total_items: source.max_total_items,
            max_total_tokens: source.max_total_tokens,
            max_single_entry_bytes: source.max_single_entry_bytes,
            reserved_tokens: source.reserved_tokens,
            mandatory_tokens: source.mandatory_tokens,
        }
    }

    pub fn domain_index_limits(&self) -> crate::domain::knowledge::activation::ActivationIndexLimits {
        crate::domain::knowledge::activation::ActivationIndexLimits {
            max_entries: self.index.max_entries,
            max_overlay_entries: self.index.max_overlay_entries,
            max_tombstones: self.index.max_tombstones,
            max_literal_patterns: self.index.max_literal_patterns,
            max_regex_patterns: self.index.max_regex_patterns,
            max_compiled_bytes: self.index.max_compiled_bytes,
            max_regex_program_bytes: self.rule.max_regex_program_bytes,
            max_macro_expansion_bytes: self.rule.max_macro_expansion_bytes,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let positive = [
            self.rule.max_primary_patterns_per_entry,
            self.rule.max_secondary_patterns_per_entry,
            self.rule.max_pattern_bytes,
            self.rule.max_regex_program_bytes,
            self.rule.max_groups_per_entry,
            self.rule.max_group_key_bytes,
            self.rule.max_macro_value_bytes,
            self.rule.max_macro_expansion_bytes,
            self.index.max_entries,
            self.index.max_overlay_entries,
            self.index.max_tombstones,
            self.index.max_literal_patterns,
            self.index.max_regex_patterns,
            self.index.max_compiled_bytes,
            self.cache.max_cached_stories,
            self.cache.max_fragments_per_story,
            self.cache.max_matches_per_fragment,
            self.cache.max_evidence_bytes_per_fragment,
            self.cache.max_total_estimated_bytes,
            self.runtime.max_scan_fragments,
            self.runtime.max_scan_bytes,
            self.runtime.max_scan_tokens as usize,
            self.runtime.max_literal_patterns,
            self.runtime.max_regex_patterns,
            self.runtime.max_pattern_matches,
            self.runtime.max_candidates_per_round,
            self.runtime.max_recursion_steps as usize,
            self.runtime.max_recursion_fragments,
            self.runtime.max_recursion_bytes,
            self.runtime.max_recursion_tokens as usize,
            self.runtime.max_activated_entries,
            self.runtime.max_depth_expansions as usize,
            self.runtime.max_external_candidates,
            self.runtime.max_evidence_per_entry,
            self.runtime.max_evidence_bytes,
            self.runtime.max_items_per_audience,
            self.runtime.max_tokens_per_audience as usize,
            self.runtime.max_total_items,
            self.runtime.max_total_tokens as usize,
            self.runtime.max_single_entry_bytes,
        ];
        if positive.into_iter().any(|value| value == 0) {
            return Err(ConfigError::Invalid("activation maximums must be positive".into()));
        }
        if self.runtime.initial_scan_depth > self.runtime.max_scan_depth {
            return Err(ConfigError::Invalid("activation initial scan depth exceeds maximum".into()));
        }
        if self.rule.max_group_key_bytes > ActivationGroupKey::MAX_BYTES {
            return Err(ConfigError::Invalid("activation group key limit exceeds type limit".into()));
        }
        if self.runtime.reserved_tokens > self.runtime.max_total_tokens
            || self.runtime.mandatory_tokens > self.runtime.max_total_tokens
            || self.runtime.reserved_tokens.saturating_add(self.runtime.mandatory_tokens)
                > self.runtime.max_total_tokens
        {
            return Err(ConfigError::Invalid("activation reserved budgets exceed total budget".into()));
        }
        if self.runtime.max_items_per_audience > self.runtime.max_total_items
            || self.runtime.max_tokens_per_audience > self.runtime.max_total_tokens
        {
            return Err(ConfigError::Invalid("activation audience budgets exceed total budget".into()));
        }
        Ok(())
    }
}

impl ActivationRuleLimitsConfig {
    pub fn limits(&self) -> ActivationRuleLimits {
        ActivationRuleLimits {
            max_primary_patterns_per_entry: self.max_primary_patterns_per_entry,
            max_secondary_patterns_per_entry: self.max_secondary_patterns_per_entry,
            max_pattern_bytes: self.max_pattern_bytes,
            max_regex_program_bytes: self.max_regex_program_bytes,
            max_groups_per_entry: self.max_groups_per_entry,
            max_group_key_bytes: self.max_group_key_bytes,
        }
    }
}
