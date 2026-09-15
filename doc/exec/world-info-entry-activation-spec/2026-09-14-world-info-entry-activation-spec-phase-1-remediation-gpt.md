# World Info Entry Activation — Phase 1 Remediation Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-14
> **Status**: Proposed
> **Source Design**: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
> **Phase**: Phase 1 Remediation — close the delta between the Phase 1 Spec and the merged implementation

---

## 1. Goal

Bring the merged activation implementation to full conformance with
[Phase 1](./2026-09-10-world-info-entry-activation-spec-phase-1-gpt.md) by adding the missing
Frozen index, fragment-match cache, provider boundary, typed error codes, and activation spans,
restoring the post-admission body-loading boundary, and deleting the remaining legacy Entity/Topic
and duplicate-path residue.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

Each item below is a verified delta against the Phase 1 Spec. Section references in parentheses
point at the Phase 1 Spec clause that is currently unsatisfied.

| ID | Delta | Phase 1 clause |
|---|---|---|
| `W1` | Add the compiled Frozen literal/regex index and enforce every `ActivationIndexLimits` field as an error | §3.6, §5.3 |
| `W2` | Add the bounded fragment-match cache (key, value, trait, LRU owner) and wire it into the coordinator | §3.6, WIA1-22/23, §5.3 |
| `W3` | Restore post-admission bounded body loading; delete eager full-body prefetch and the duplicate index preparation | §3.8, WIA1-24 |
| `W4` | Add the definition-only `ActivationSeedProvider` boundary | §3.13, WIA1-30 |
| `W5` | Complete `ActivationError` and expose all eleven stable Turn error codes | §3.16, §4.1 |
| `W6` | Emit the five `knowledge.activation.*` spans with the §3.17 field set | §3.17, §4.3 |
| `W7` | Validate `KnowledgeActivationRule` at the asset-import boundary and enforce every `ActivationRuleLimitsConfig` field | §4.1, §5.1 |
| `W8` | Delete remaining legacy Entity/Topic types and the duplicate scan-buffer/matcher paths | §3.1, WIA1-02, §5.6 |
| `W9` | Align `TurnExecutionContext::replace_activation`, `ActivationRuleVersion`, and `FactSeed`/`RumorSeed.retrieval_hint` with their declared contracts | §3.2, §3.4, §3.12 |
| `W10` | Add the missing activation test directories and the `context_preparation_retrieval_tests` target | §3.1, §5.7 |
| `W11` | Mark superseded Entity/Topic retrieval documentation | §5.6 |

### 2.2 Non-Goals

- Does not change any activation semantics that already conform: matching, secondary logic,
  groups, deterministic probability, timing, recursion, depth expansion, ranking, or budget
  arithmetic. Behavior observable through `ActivationResult` stays byte-equivalent except where
  §4 of this spec states otherwise.
- Does not implement a concrete provider, embedding model, vector index, or BM25 index.
- Does not add provider/model/threshold/top-k fields to any asset, Planner, or Turn contract.
- Does not add a cache table, cross-process cache, or persisted compiled regex program.
- Does not change migration `0023_world_info_entry_activation.sql`, the committed SQLite schema,
  `TurnCommitSpec` atomicity, or the overlay-version increment rule; those already conform.
- Does not change Phase 2 Preview request/response shapes, routes, or the static web client.
- Does not add a World Info editor, Pack mutation, or authoring surface.
- Does not move Memory into the Role aggregate.
- Does not renumber, restate, or weaken `WIA1-01` … `WIA1-30`; they remain in force.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, or
  dual-write logic.
- Old types / functions / modules superseded by this spec MUST be deleted, not deprecated
  (`R-REFACTOR-01/02`).
- No mid-state "both systems coexist" phase. In particular `W3` and `W8` remove a path in the
  same change that adds its replacement.
- `KnowledgeActivationEngine` stays synchronous and pure over bounded inputs. Index composition,
  regex compilation, cache access, and body loading belong to `KnowledgeActivationCoordinator`
  and the persistence layer.
- Domain activation modules MUST NOT import `context`, `persistence`, `runtime`, `api`, or
  `config` modules (`R-LAYER-01`). The current `config -> context::activation::scan_buffer`
  import is a violation and is deleted by `W2`.
- Every new collection, cache, pattern set, and byte total has a positive trusted limit;
  `0` never means unlimited (`WIA1-14`).
- No lock or DB transaction crosses `.await`; no trace emission or I/O occurs while holding a
  write guard (`R-CONC-01`, `R-CONC-03`).
- Code follows `AGENTS.md`: index-only `mod.rs`, directory modules, `tests/<source>_tests.rs`,
  no ordinary code comments, compact imports, `forbid(unsafe_code)`, format and Clippy clean.

### 2.4 Required Implementation Order

1. `W8` deletions and `W9` contract alignment (smallest blast radius, unblocks static checks).
2. `W7` asset-boundary validation and rule-limit enforcement.
3. `W1` Frozen index and compiled matcher.
4. `W2` fragment-match cache on top of the compiled index.
5. `W3` body-loading boundary (depends on `W1` metadata-only snapshots).
6. `W5` error codes, then `W4` provider boundary.
7. `W6` spans.
8. `W10` tests, then `W11` documentation.

---

## 3. Contracts

### 3.1 Target File and Module Layout

```text
crates/aise/src/
├── domain/knowledge/activation/
│   ├── mod.rs
│   ├── rule.rs
│   ├── state.rs
│   ├── scan.rs
│   ├── contracts.rs
│   ├── engine.rs
│   ├── evidence.rs                 (W6: add)
│   ├── provider.rs                 (W4: add)
│   └── tests/                      (W10: add)
│       ├── rule_tests.rs
│       ├── scan_tests.rs
│       ├── engine_tests.rs
│       └── provider_tests.rs
├── context/activation/
│   ├── mod.rs
│   ├── coordinator.rs
│   ├── index.rs                    (W1: add)
│   ├── fragment_cache.rs           (W2: add)
│   ├── preview.rs
│   ├── preview_service.rs
│   └── tests/                      (W10: add)
│       ├── index_tests.rs
│       ├── fragment_cache_tests.rs
│       └── coordinator_tests.rs
└── persistence/
    ├── activation_index_port.rs
    ├── activation_timed_state_port.rs
    └── sqlite_activation.rs

crates/aise/tests/
└── context_preparation_retrieval_tests.rs   (W10: add)
```

Required deletions:

```text
crates/aise/src/domain/asset/entity.rs
crates/aise/src/domain/asset/text_matcher.rs
crates/aise/src/context/activation/matcher.rs
crates/aise/src/context/activation/scan_buffer.rs
```

`domain/knowledge/activation/scan.rs` is the single surviving Scan Buffer module.

### 3.2 W1 — Frozen Index and Compiled Matcher

`crates/aise/src/context/activation/index.rs`:

```rust
pub const MATCHER_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompiledPatternRef {
    pub source_ordinal: u32,
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
}

#[derive(Debug)]
pub struct FrozenLiteralIndex {
    entries: Vec<KnowledgeSourceId>,
    normalized: Vec<Box<str>>,
    owners: Vec<CompiledPatternRef>,
    case_sensitive: Vec<bool>,
    whole_word: Vec<bool>,
    estimated_bytes: usize,
}

impl FrozenLiteralIndex {
    pub fn match_fragment(&self, text: &str) -> Vec<FragmentPatternMatch>;
    pub fn pattern_count(&self) -> usize;
    pub fn estimated_bytes(&self) -> usize;
}

#[derive(Debug)]
pub struct FrozenRegexSet {
    programs: Vec<Regex>,
    owners: Vec<CompiledPatternRef>,
    estimated_bytes: usize,
}

impl FrozenRegexSet {
    pub fn match_fragment(&self, text: &str) -> Vec<FragmentPatternMatch>;
    pub fn pattern_count(&self) -> usize;
    pub fn estimated_bytes(&self) -> usize;
}

pub struct FrozenPackIndex {
    pub pack_digest: Sha256Digest,
    pub matcher_version: u32,
    pub literal_index: FrozenLiteralIndex,
    pub regex_set: FrozenRegexSet,
    pub constant_entries: Vec<KnowledgeSourceId>,
    pub metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
}

pub struct FrozenPackIndexCache {
    inner: Mutex<LruMap<Sha256Digest, Arc<FrozenPackIndex>>>,
    max_packs: usize,
    max_total_estimated_bytes: usize,
}

impl FrozenPackIndexCache {
    pub fn get(&self, pack_digest: &Sha256Digest) -> Option<Arc<FrozenPackIndex>>;
    pub fn insert(&self, index: Arc<FrozenPackIndex>) -> Result<(), ActivationError>;
}

pub fn build_frozen_pack_index(
    pack_digest: Sha256Digest,
    entries: Vec<ActivationEntryMetadata>,
    macros: &ActivationMacroValues,
    limits: ActivationIndexLimits,
) -> Result<FrozenPackIndex, ActivationError>;

pub fn compose_index_snapshot(
    frozen: Arc<FrozenPackIndex>,
    overlay: ActivationOverlayIndex,
    reference: ActivationIndexSnapshotRef,
    limits: ActivationIndexLimits,
) -> Result<ActivationIndexSnapshot, ActivationError>;
```

`ActivationOverlayIndex` carries runtime additions/updates plus tombstones only:

```rust
#[derive(Debug, Clone, Default)]
pub struct ActivationOverlayIndex {
    pub overlay_version: u64,
    pub upserts: Vec<ActivationEntryMetadata>,
    pub tombstones: BTreeSet<KnowledgeSourceId>,
}
```

`ActivationIndexSnapshot` reaches the Phase 1 §3.6 shape:

```rust
pub struct ActivationIndexSnapshot {
    pub reference: ActivationIndexSnapshotRef,
    pub literal_index: Arc<FrozenLiteralIndex>,
    pub regex_set: Arc<FrozenRegexSet>,
    pub overlay_literal_index: FrozenLiteralIndex,
    pub overlay_regex_set: FrozenRegexSet,
    pub constant_entries: Vec<KnowledgeSourceId>,
    pub metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
}
```

`ActivationExecutionInput` and the `ActivationEntryInput` body vector are **removed** from
`ActivationIndexSnapshot`; see §3.4.

`ActivationIndexPort` keeps its Phase 1 signature. `SqliteActivationIndexReader` returns the
composed snapshot and MUST NOT select `knowledge_entries.content`.

### 3.3 W2 — Fragment Match Cache

`crates/aise/src/context/activation/fragment_cache.rs` owns the whole contract. The partial
`FragmentMatchCacheKey` in `crates/aise/src/config/activation.rs:245` is deleted.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentMatchCacheKey {
    pub fragment_id: ScanFragmentId,
    pub fragment_kind: ScanFragmentKind,
    pub content_hash: Sha256Digest,
    pub pack_digest: Sha256Digest,
    pub overlay_version: u64,
    pub matcher_version: u32,
    pub macro_digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct FragmentPatternMatch {
    pub source_id: KnowledgeSourceId,
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_fragment_order: u32,
    pub match_count: u16,
    pub group_score_contribution: u16,
}

#[derive(Debug, Clone)]
pub struct FragmentMatchCacheValue {
    pub matches: Vec<FragmentPatternMatch>,
}

pub trait FragmentMatchCache: Send + Sync {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<Arc<FragmentMatchCacheValue>>;
    fn insert(
        &self,
        key: FragmentMatchCacheKey,
        value: FragmentMatchCacheValue,
    ) -> Result<(), ActivationError>;
}

pub struct LruFragmentMatchCache {
    inner: Mutex<LruFragmentMatchState>,
    limits: FragmentMatchCacheLimits,
}

impl LruFragmentMatchCache {
    pub fn new(limits: FragmentMatchCacheLimits) -> Self;
    pub fn stats(&self) -> FragmentMatchCacheStats;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FragmentMatchCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insert_rejections: u64,
    pub evictions: u64,
}
```

`macro_digest` is the SHA-256 of canonical serialized `ActivationMacroValues`.
`FragmentMatchCacheStats` is telemetry only and never influences a decision.

### 3.4 W3 — Post-Admission Body Loading

`KnowledgeActivationEngine::run` becomes body-free and returns admitted IDs to the caller for
bounded loading. Recursion is driven by a caller-supplied round callback that the coordinator
implements with `find_by_source_ids`.

```rust
pub struct ActivationRoundOutcome {
    pub admitted: Vec<KnowledgeSourceId>,
    pub state: ActivationMachineState,
}

pub struct ActivationRecursionInput {
    pub bodies: Vec<ActivationEntryBody>,
}

#[derive(Debug, Clone)]
pub struct ActivationEntryBody {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub token_cost: u64,
    pub body: BoundedText,
}

pub struct KnowledgeActivationSession<'a> {
    /* bounded engine state for one logical Turn */
}

impl KnowledgeActivationEngine {
    pub fn start(
        &self,
        request: ActivationRequest<'_>,
    ) -> Result<KnowledgeActivationSession<'_>, ActivationError>;
}

impl<'a> KnowledgeActivationSession<'a> {
    pub fn next_round(&mut self) -> Result<Option<ActivationRoundOutcome>, ActivationError>;
    pub fn supply_bodies(
        &mut self,
        input: ActivationRecursionInput,
    ) -> Result<(), ActivationError>;
    pub fn drop_admitted(&mut self, source_id: &KnowledgeSourceId) -> Result<(), ActivationError>;
    pub fn finish(self) -> Result<ActivationResult, ActivationError>;
}
```

Coordinator drive loop:

```rust
impl KnowledgeActivationCoordinator {
    pub async fn run(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        scan_buffer: &ActivationScanBuffer,
        macros: ActivationMacroValues,
        story_id: &StoryId,
        turn_number: TurnNumber,
        generation_trigger: GenerationTrigger,
        mode: ActivationRunMode,
        external_seeds: &[ExternalActivationSeed],
        continuation: Option<ActivationContinuation>,
    ) -> Result<ActivationResult, ActivationError>;

    pub async fn prepare_index(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: ActivationMacroValues,
    ) -> Result<Arc<ActivationIndexSnapshot>, ActivationError>;
}
```

Deleted in the same change:

- `ActivationExecutionInput`, `ActivationEntryInput`, `ActivationIndexSnapshot::with_execution_input`
- `ActivationError::MissingExecutionInput`, `ActivationError::MissingEntryInput`
- the `activation_execution_body_incomplete` constraint at
  `crates/aise/src/context/activation/coordinator.rs:119-123`
- the unimplemented `ActivationBodyLoader` trait at
  `crates/aise/src/context/activation/coordinator.rs:176-182`
- the second `prepare_index` call in `crates/aise/src/context/retrieval_pipeline.rs`

`BaselineContextBuilder` prepares the index exactly once per Turn and passes the same
`Arc<ActivationIndexSnapshot>` to `ContextRetrievalPipeline` through `TurnExecutionContext`.

### 3.5 W4 — Provider Boundary

`crates/aise/src/domain/knowledge/activation/provider.rs`, definitions only:

```rust
#[derive(Debug, Clone)]
pub struct ActivationSeedRequest<'a> {
    pub knowledge_snapshot: &'a KnowledgeSnapshotRef,
    pub scan_buffer: &'a ActivationScanBuffer,
    pub query_text: Option<&'a BoundedText>,
    pub allowed_kinds: &'a [KnowledgeKind],
    pub delivery: &'a KnowledgeDelivery,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ProviderActivationCandidate {
    pub source_id: KnowledgeSourceId,
    pub provider_rank: u32,
    pub evidence: ActivationEvidence,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivationProviderError {
    #[error("activation provider request is invalid: {code}")]
    InvalidRequest { code: &'static str },
    #[error("activation provider limit exceeded")]
    LimitExceeded,
    #[error("activation provider is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait ActivationSeedProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    async fn candidates(
        &self,
        request: ActivationSeedRequest<'_>,
    ) -> Result<Vec<ProviderActivationCandidate>, ActivationProviderError>;
}
```

No implementation, registry, config block, or composition-root registration is added.

### 3.6 W5 — Errors and Turn Codes

```rust
#[derive(Debug, thiserror::Error)]
pub enum ActivationError {
    #[error("activation rule is invalid: {code}")]
    InvalidRule { code: &'static str },
    #[error("activation regex is invalid or unsupported")]
    InvalidRegex,
    #[error("activation index version mismatch")]
    IndexVersionMismatch,
    #[error("activation snapshot mismatch")]
    SnapshotMismatch,
    #[error("activation continuation mismatch")]
    ContinuationMismatch,
    #[error("activation work limit exceeded: {limit}")]
    WorkLimitExceeded { limit: &'static str },
    #[error("activation recursion step limit reached")]
    RecursionLimitReached,
    #[error("mandatory knowledge budget exceeded")]
    MandatoryBudgetExceeded,
    #[error("external activation target is unauthorized")]
    ExternalTargetUnauthorized,
    #[error("activation timed state is inconsistent")]
    TimedStateInconsistent,
    #[error("activation provider failed: {provider}")]
    ProviderFailure { provider: &'static str },
    #[error("knowledge activation store operation failed")]
    Store(StoreError),
}

impl ActivationError {
    pub fn code(&self) -> &'static str;
}
```

`ContextError` gains one activation-carrying variant and loses two legacy variants:

```rust
pub enum ContextError {
    Activation(ActivationError),
    /* ... existing non-legacy variants ... */
}
```

Deleted: `ContextError::SignalLimitExceeded` (`crates/aise/src/context/error.rs:11`) and
`ContextError::InvalidRetrieverSet` (`crates/aise/src/context/error.rs:17`), plus their code
mappings at `crates/aise/src/context/error.rs:40,42` and the match arm at
`crates/aise/src/context/retrieval_pipeline.rs:406`.

Exact code mapping:

| `ActivationError` variant | Turn error code |
|---|---|
| `InvalidRule` | `activation_rule_invalid` |
| `InvalidRegex` | `activation_regex_invalid` |
| `IndexVersionMismatch` | `activation_index_mismatch` |
| `SnapshotMismatch` | `activation_snapshot_conflict` |
| `ContinuationMismatch` | `activation_continuation_mismatch` |
| `WorkLimitExceeded` | `activation_work_limit` |
| `RecursionLimitReached` | `activation_recursion_limit` |
| `MandatoryBudgetExceeded` | `activation_mandatory_budget` |
| `ExternalTargetUnauthorized` | `activation_target_unauthorized` |
| `TimedStateInconsistent` | `activation_timed_state_invalid` |
| `ProviderFailure` | `activation_provider_failed` |
| `Store` | existing `store_unavailable` mapping |

`StoreError::ConstraintViolation { constraint: error.to_string() }` at
`crates/aise/src/context/activation/coordinator.rs:83-85` is deleted; the coordinator returns
`ActivationError` and wraps store failures in `ActivationError::Store`.

Budget rejection semantics are split so `admit_deliveries` no longer reports every overflow as
mandatory:

```rust
pub enum ActivationAdmissionOutcome {
    Admitted,
    Rejected(ActivationRejectionReason),
    MandatoryOverflow,
}
```

`ActivationError::MandatoryBudgetExceeded` is produced only for
`ActivationBudgetClass::Mandatory` candidates and authorized external seeds with
`mandatory = true`. Normal and reserved overflow records `ActivationRejectionReason::Budget`
(item/token exhaustion) or `ActivationRejectionReason::WorkLimit` (work exhaustion).

### 3.7 W6 — Observability

Spans and required fields:

```text
knowledge.activation.prepare {
    story_id, turn_number, base_revision, pack_digest, overlay_version,
    matcher_version, generation_trigger, mode,
    index_entries, overlay_entries, tombstones,
    literal_patterns, regex_patterns, compiled_bytes,
    frozen_cache_hit, timed_state_entries, status, error_code, latency_ms
}

knowledge.activation.round {
    story_id, turn_number, state, round, recursion_level, scan_depth,
    scan_fragments, scan_bytes, scan_tokens,
    literal_matches, regex_matches, pattern_matches,
    cache_hits, cache_misses,
    candidates, activated, rejected,
    rejected_disabled, rejected_scope_mismatch, rejected_delayed,
    rejected_cooldown, rejected_recursion_excluded,
    rejected_recursion_level_locked, rejected_secondary_condition,
    rejected_group_loser, rejected_probability, rejected_budget,
    rejected_duplicate, rejected_work_limit,
    knowledge_tokens, stop_reason, status, error_code
}

knowledge.activation.resume {
    story_id, turn_number, base_revision, pack_digest, overlay_version,
    external_seeds, authorized_seeds, newly_activated,
    consumed_items, consumed_tokens, status, error_code, latency_ms
}

knowledge.activation.provider {
    story_id, turn_number, provider_id, requested_limit,
    candidate_count, status, error_code, latency_ms
}

knowledge.activation.commit {
    story_id, turn_number, timed_upserts, timed_deletes,
    overlay_version_before, overlay_version_after, status, error_code
}
```

`crates/aise/src/domain/knowledge/activation/evidence.rs` owns the bounded projection used by
span fields and by Phase 2 evidence:

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct ActivationRejectionCounts {
    /* one u32 per ActivationRejectionReason variant */
}

impl ActivationRejectionCounts {
    pub fn from_summary(summary: &BTreeMap<ActivationRejectionReason, u32>) -> Self;
}

pub fn bounded_evidence_digest(evidence: &[ActivationEvidence], max_bytes: usize) -> Sha256Digest;
```

`knowledge.activation.provider` is emitted by the coordinator only when a provider is registered;
Phase 1 registers none, so the span exists and stays unused.

### 3.8 W7 — Asset Boundary Validation

```rust
impl KnowledgeActivationRule {
    pub fn validate(
        &self,
        limits: ActivationRuleLimits,
    ) -> Result<(), ActivationRuleValidationError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ActivationRuleLimits {
    pub max_primary_patterns_per_entry: usize,
    pub max_secondary_patterns_per_entry: usize,
    pub max_pattern_bytes: usize,
    pub max_regex_program_bytes: usize,
    pub max_groups_per_entry: usize,
    pub max_group_key_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivationRuleValidationError {
    /* existing variants */
    #[error("activation rule has too many primary patterns")]
    TooManyPrimaryPatterns,
    #[error("activation rule has too many secondary patterns")]
    TooManySecondaryPatterns,
    #[error("activation pattern exceeds its byte budget")]
    PatternTooLong,
    #[error("activation regex program exceeds its byte budget")]
    RegexProgramTooLarge,
    #[error("activation rule has too many groups")]
    TooManyGroups,
    #[error("activation group key exceeds its byte budget")]
    GroupKeyTooLong,
}
```

The `validate(&self, max_groups: usize)` signature at
`crates/aise/src/domain/knowledge/activation/rule.rs:225` is replaced. Both existing call sites
that pass `rule.selection.groups.len()` are replaced with the configured limits.

`ActivationRuleLimitsConfig::limits(&self) -> ActivationRuleLimits` provides the value.
`StoryInstanceFactory` validates every `FactSeed.activation` and `RumorSeed.activation` before
constructing `WorldFact`/`SharedRumor` (currently cloned unvalidated at
`crates/aise/src/story/instance_factory.rs:468` and `:510`), and Pack import validates before
persistence. Validation failure surfaces as an `AssetValidationCode`, not a Turn error.

`build_frozen_pack_index` re-validates and compiles; a rule that fails there returns
`ActivationError::InvalidRule` or `ActivationError::InvalidRegex` and no Turn proceeds.

### 3.9 W9 — Contract Alignment

```rust
impl TurnExecutionContext {
    pub fn replace_activation(
        &mut self,
        activation: PreparedActivation,
    ) -> Result<(), TurnExecutionError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationRuleVersion(Sha256Digest);

impl ActivationRuleVersion {
    pub fn from_rule(rule: &KnowledgeActivationRule) -> Self;
    pub fn as_digest(&self) -> &Sha256Digest;
}
```

`replace_activation` (currently `crates/aise/src/turn/turn_context.rs:247`) takes
`PreparedActivation`, validates the post-Planner retrieval phase with
`expect_phase(TurnPhase::Planned)`, and returns `Result`. Its call site in
`crates/aise/src/context/retrieval_pipeline.rs` propagates the error.

`FactSeed.retrieval_hint` and `RumorSeed.retrieval_hint`
(`crates/aise/src/domain/asset/world_book.rs:40,51`) change from `Option<BoundedText>` to
`Option<RetrievalHint>`.

---

## 4. Behavior Rules

Rules below extend `WIA1-01` … `WIA1-30`, which remain in force unchanged.

1. **WIA1R-01 — Compile Once**: Every literal and regex pattern is normalized and compiled
   exactly once per `(pack_digest, matcher_version, macro_digest)` during Frozen index
   construction. No `Regex::new`, `RegexBuilder`, or literal normalization call occurs inside a
   round, per pattern, per fragment, or per candidate.
2. **WIA1R-02 — Index Limits Are Errors**: Exceeding `max_entries`, `max_overlay_entries`,
   `max_tombstones`, `max_literal_patterns`, `max_regex_patterns`, or `max_compiled_bytes`
   returns `ActivationError::WorkLimitExceeded { limit }`. No SQL `LIMIT`, `take`, or truncation
   silently drops an Entry, pattern, or tombstone.
3. **WIA1R-03 — Index Is Metadata Only**: `ActivationIndexSnapshot` contains no Fact, Rumor, or
   Memory body text. `SqliteActivationIndexReader` does not select a content column.
4. **WIA1R-04 — Frozen Reuse**: A second Turn on the same `pack_digest` and `matcher_version`
   reuses the cached `Arc<FrozenPackIndex>` and performs zero pattern compilation. A digest,
   matcher-version, or overlay-version change forces recomposition; overlay recomposition never
   recompiles Frozen Pack patterns.
5. **WIA1R-05 — Cache Neutrality**: `ActivationResult` is byte-equivalent under cache hit, cache
   miss, insert rejection, and eviction. `FragmentMatchCacheStats` never feeds a decision.
6. **WIA1R-06 — Cache Invalidation**: A change in `content_hash`, `pack_digest`,
   `overlay_version`, `matcher_version`, or `macro_digest` yields a miss.
7. **WIA1R-07 — Cache Bounds**: Insertion that would exceed `max_cached_stories`,
   `max_fragments_per_story`, `max_matches_per_fragment`, `max_evidence_bytes_per_fragment`, or
   `max_total_estimated_bytes` evicts by LRU or rejects; it never grows past the bound and never
   returns an error to the Turn.
8. **WIA1R-08 — Cache Content Boundary**: The cache stores only the `FragmentPatternMatch` fields
   in §3.3. It never stores Entry content, activation/rejection outcome, group winner,
   probability result, budget decision, timed state, or continuation.
9. **WIA1R-09 — Post-Admission Loading**: Bodies load only for `ActivationRoundOutcome.admitted`
   IDs. Total body reads per Turn are bounded by `max_activated_entries`. Loading all Pack
   Fact/Rumor bodies, or loading any body before admission, is a defect.
10. **WIA1R-10 — Single Index Preparation**: One logical Turn calls `prepare_index` exactly once.
    `ContextRetrievalPipeline` reuses the Baseline snapshot and never rebuilds or reloads it.
11. **WIA1R-11 — Body Load Failure**: A non-mandatory body-load failure calls `drop_admitted`,
    records `ActivationRejectionReason::Budget`, and continues. A mandatory body-load failure
    returns `ActivationError::MandatoryBudgetExceeded` and aborts the Turn.
12. **WIA1R-12 — Kind Validation**: A loaded body whose `KnowledgeKind` or `KnowledgeSourceId`
    disagrees with index metadata returns `ActivationError::SnapshotMismatch`.
13. **WIA1R-13 — Typed Codes**: Every activation failure reaching the Turn boundary carries its
    §3.6 code. Collapsing an activation failure into `store_error`,
    `context_baseline_limit`, or `retrieval_candidate_limit` is a defect.
14. **WIA1R-14 — Mandatory Precision**: `MandatoryBudgetExceeded` is returned only for mandatory
    candidates; normal and reserved overflow is a rejection reason, not a Turn abort.
15. **WIA1R-15 — Span Completeness**: Each activation execution emits one `prepare` span, one
    `round` span per bounded round, and one `resume` span for post-Planner resumption. Resume
    never emits a second `prepare` span.
16. **WIA1R-16 — Telemetry Redaction**: No span, log, or metric records Story text, Summary text,
    Player Contribution, pattern text, regex text, Entry content, Memory content, Prompt content,
    or raw LLM output.
17. **WIA1R-17 — Validate Before Persist**: No `KnowledgeActivationRule` is persisted or promoted
    into `WorldFact`/`SharedRumor` without a `validate(limits)` call using configured
    `ActivationRuleLimitsConfig` values. Passing `groups.len()` as the group limit is a defect.
18. **WIA1R-18 — Rule Limits Enforced**: Each of the six `ActivationRuleLimitsConfig` fields has
    at least one rejecting input. A configured field with no enforcement path is a defect.
19. **WIA1R-19 — Provider Definition Only**: The workspace contains the `ActivationSeedProvider`
    trait and its request/candidate/error types and zero implementations, registries, config
    blocks, or composition-root registrations.
20. **WIA1R-20 — Single Path**: Exactly one Scan Buffer module, one literal matcher, and one
    regex matcher exist. A second same-named type, a dead matcher module, or an unreferenced
    duplicate is a defect.
21. **WIA1R-21 — No Legacy Residue**: No `TopicKey`, `TopicDefinition`, `KnowledgeEntity`,
    `TextMatcher`, `EntitySignal`, `TopicSignal`, `RetrievalSignals`, or `KnowledgeIndexMatch`
    symbol exists in `crates`, `examples`, or `config`.
22. **WIA1R-22 — Layer Direction**: `crates/aise/src/config` contains no `use crate::context::…`
    or `use crate::persistence::…` import. Activation limit types live in `config`; activation
    runtime types live in `domain` and `context`.
23. **WIA1R-23 — Phase Guard**: `replace_activation` rejects a call outside
    `TurnPhase::Planned` with `TurnExecutionError` and its caller propagates it.
24. **WIA1R-24 — Semantic Neutrality**: For an identical Story, Pack, overlay, timed state, scan
    buffer, trigger, seeds, and configuration, activated IDs, deliveries, order, ranks, token
    costs, evidence, rejection summary, stop reason, and pending delta are byte-identical before
    and after this remediation, except where `WIA1R-14` reclassifies a non-mandatory overflow.

### 4.1 Error Handling

- On an index limit breach, return `ActivationError::WorkLimitExceeded { limit }` where `limit`
  is the exact field name (`"max_entries"`, `"max_overlay_entries"`, `"max_tombstones"`,
  `"max_literal_patterns"`, `"max_regex_patterns"`, `"max_compiled_bytes"`).
- On a cache insertion that cannot satisfy `FragmentMatchCacheLimits` after eviction, increment
  `insert_rejections` and return `Ok(())`; never surface a cache failure to the Turn.
- On a rule that violates `ActivationRuleLimits` at asset import, return the matching
  `ActivationRuleValidationError` mapped to an `AssetValidationCode`; the Pack is not persisted.
- On a rule that fails during `build_frozen_pack_index`, return `ActivationError::InvalidRule`
  or `ActivationError::InvalidRegex` before any body read.
- `StoreError` reaching activation is wrapped in `ActivationError::Store`; the raw
  `StoreError::ConstraintViolation { constraint: error.to_string() }` pattern is removed.
- Asset, LLM, Story, and request paths contain no `unwrap`, `expect`, or panic.

### 4.2 Concurrency

- `FrozenPackIndexCache` and `LruFragmentMatchCache` use `std::sync::Mutex`. The guard scope is
  synchronous, contains no `.await`, no tracing emission, and no I/O (`R-CONC-01`, `R-CONC-03`).
- The engine session is `!Send`-tolerant synchronous state; the coordinator holds no lock across
  the `next_round` / body-load / `supply_bodies` cycle.
- Body loads use one bounded `find_by_source_ids` call per round, capped by
  `max_activated_entries`; no per-Entry task, detached future, unbounded channel, or
  `join_all` fan-out is added.
- Index and timed-state reads use short transactions closed before matching.
- No embedding or LLM call is added; a future provider call must pass the shared LLM concurrency
  limiter (`R-CONC-04`).

### 4.3 Observability

- Zero-match, scope rejection, group rejection, probability rejection, budget trimming, work
  trimming, depth exhaustion, and recursion stop are distinguishable by the `round` span
  rejection counters plus `stop_reason`.
- `frozen_cache_hit`, `cache_hits`, and `cache_misses` make compile and match reuse observable.
- Identifiers use structured fields, never string interpolation (`R-OBS-04`).
- Activation error paths always populate `status` and `error_code`.

---

## 5. Acceptance Criteria

### 5.1 Frozen Index and Matcher (W1)

- [ ] `FrozenLiteralIndex`, `FrozenRegexSet`, `FrozenPackIndex`, `FrozenPackIndexCache`,
      `ActivationOverlayIndex`, and `MATCHER_VERSION` exist in
      `crates/aise/src/context/activation/index.rs` and match §3.2.
- [ ] `ActivationIndexSnapshot` matches §3.2; `ActivationExecutionInput`, `ActivationEntryInput`,
      and `with_execution_input` return zero matches under
      `rg -n 'ActivationExecutionInput|ActivationEntryInput|with_execution_input' crates`.
- [ ] `rg -n 'Regex::new|RegexBuilder' crates/aise/src` shows matches only inside
      `context/activation/index.rs`.
- [ ] A two-Turn test on one Pack digest asserts the second Turn compiles zero patterns and
      reuses the cached `Arc<FrozenPackIndex>`.
- [ ] Six tests, one per `ActivationIndexLimits` field, assert
      `ActivationError::WorkLimitExceeded { limit }` with the exact field name and assert no
      truncated result is returned.
- [ ] `rg -n 'LIMIT' crates/aise/src/persistence/sqlite_activation.rs` shows no Entry-count cap
      used in place of a limit error.
- [ ] An overlay-only change test asserts recomposition without Frozen recompilation.
- [ ] `cargo test -p aise context::activation::tests::index_tests` passes.

### 5.2 Fragment Cache (W2)

- [ ] `FragmentMatchCacheKey`, `FragmentMatchCacheValue`, `FragmentPatternMatch`,
      `FragmentMatchCache`, and `LruFragmentMatchCache` exist in
      `crates/aise/src/context/activation/fragment_cache.rs` and match §3.3.
- [ ] `FragmentMatchCacheKey.fragment_id` is `ScanFragmentId` and the type derives `Hash`.
- [ ] `rg -n 'FragmentMatchCacheKey' crates/aise/src/config` returns zero matches.
- [ ] `rg -n 'use crate::context|use crate::persistence' crates/aise/src/config` returns zero
      matches.
- [ ] Cache hit and cache miss produce byte-identical `ActivationResult` for the same request.
- [ ] Five tests each mutate exactly one of `content_hash`, `pack_digest`, `overlay_version`,
      `matcher_version`, `macro_digest` and assert a miss.
- [ ] Five tests, one per `FragmentMatchCacheLimits` field, assert the bound is never exceeded and
      that the Turn still succeeds.
- [ ] A write-failure/eviction test asserts identical activation output and a nonzero
      `insert_rejections` or `evictions` count.
- [ ] `cargo test -p aise context::activation::tests::fragment_cache_tests` passes.

### 5.3 Body Loading Boundary (W3)

- [ ] `KnowledgeActivationSession` matches §3.4 and `KnowledgeActivationEngine::run` no longer
      accepts or returns body text.
- [ ] A Pack with 200 Fact/Rumor Entries where 3 activate asserts exactly one
      `find_by_source_ids` call with exactly 3 Source IDs, verified by a counting mock.
- [ ] A recursion test asserts one bounded `find_by_source_ids` call per round with only that
      round's admitted IDs.
- [ ] `rg -n 'ActivationBodyLoader|activation_execution_body_incomplete' crates` returns zero
      matches.
- [ ] `rg -n 'prepare_index' crates/aise/src` shows exactly one call site outside
      `coordinator.rs` and `preview_service.rs`.
- [ ] A full Turn test asserts exactly one `prepare_index` invocation via a counting mock.
- [ ] A non-mandatory body-load failure test asserts the Turn succeeds with a `budget` rejection;
      a mandatory failure test asserts `activation_mandatory_budget`.
- [ ] A kind-mismatch body test asserts `activation_snapshot_conflict`.

### 5.4 Provider Boundary (W4)

- [ ] `ActivationSeedProvider`, `ActivationSeedRequest`, `ProviderActivationCandidate`, and
      `ActivationProviderError` exist in
      `crates/aise/src/domain/knowledge/activation/provider.rs` and match §3.5.
- [ ] `rg -n 'impl ActivationSeedProvider' crates` returns matches only inside
      `domain/knowledge/activation/tests/provider_tests.rs`.
- [ ] `rg -n 'provider' crates/aise/src/config` returns zero activation-provider matches.
- [ ] A compile-only test constructs `ActivationSeedRequest` and asserts the trait is object-safe.

### 5.5 Errors and Codes (W5)

- [ ] `ActivationError` matches §3.6 exactly, including `ProviderFailure` and `Store`, and
      excluding `MissingExecutionInput` and `MissingEntryInput`.
- [ ] `ActivationError::code()` returns each of the eleven §3.6 codes; a table-driven test covers
      every variant.
- [ ] `rg -n 'activation_regex_invalid|activation_index_mismatch|activation_snapshot_conflict|activation_continuation_mismatch|activation_work_limit|activation_recursion_limit|activation_mandatory_budget|activation_target_unauthorized|activation_timed_state_invalid|activation_provider_failed' crates/aise/src`
      returns at least one match per code.
- [ ] `rg -n 'SignalLimitExceeded|InvalidRetrieverSet|context_baseline_limit|retrieval_candidate_limit' crates`
      returns zero matches.
- [ ] `rg -n 'StoreError::ConstraintViolation' crates/aise/src/context/activation` returns zero
      matches.
- [ ] An end-to-end Turn test per code asserts the surfaced Turn error code, proving no collapse
      into `store_error`.
- [ ] A normal-class token-overflow test asserts `ActivationRejectionReason::Budget` and Turn
      success; a mandatory-class test asserts `activation_mandatory_budget`.

### 5.6 Observability (W6)

- [ ] `rg -n 'knowledge\.activation\.(prepare|round|resume|provider|commit)' crates/aise/src`
      returns at least one span macro per name.
- [ ] A span-capture test asserts every §3.7 field is present for a successful Turn and for each
      failing Turn.
- [ ] A resume test asserts one `prepare` span and one `resume` span, never two `prepare` spans.
- [ ] A redaction test asserts no captured span field contains Player Contribution, Story text,
      pattern text, regex text, or Entry content.
- [ ] `knowledge.activation.commit` records `timed_upserts`, `timed_deletes`, and both overlay
      versions on a successful commit.

### 5.7 Asset Validation (W7)

- [ ] `KnowledgeActivationRule::validate(limits: ActivationRuleLimits)` matches §3.8 and no call
      site passes `selection.groups.len()`.
- [ ] `ActivationRuleLimitsConfig::limits()` exists and is used by `StoryInstanceFactory`, Pack
      import, dynamic extraction, and `build_frozen_pack_index`.
- [ ] Six rejection tests cover `max_primary_patterns_per_entry`,
      `max_secondary_patterns_per_entry`, `max_pattern_bytes`, `max_regex_program_bytes`,
      `max_groups_per_entry`, and `max_group_key_bytes`.
- [ ] An import test with an invalid regex asserts a validation failure and that no
      `story_packs` or `knowledge_entries` row is written.
- [ ] An import test with an invalid rule asserts an `AssetValidationCode`, not a Turn error code.
- [ ] `cargo test -p aise --test asset_import_tests` passes.

### 5.8 Legacy Removal (W8)

- [ ] `rg -n '\bTopicKey\b|\bTopicDefinition\b|\bKnowledgeEntity\b|\bTextMatcher\b|\bEntitySignal\b|\bTopicSignal\b|\bRetrievalSignals\b|\bKnowledgeIndexMatch\b' crates examples config`
      returns zero matches.
- [ ] `crates/aise/src/domain/asset/entity.rs`, `crates/aise/src/domain/asset/text_matcher.rs`,
      `crates/aise/src/context/activation/matcher.rs`, and
      `crates/aise/src/context/activation/scan_buffer.rs` do not exist.
- [ ] `rg -n 'ScanFragmentKind|ActivationScanBuffer' crates/aise/src` resolves to
      `domain/knowledge/activation/scan.rs` only.
- [ ] `rg -n 'match_topics|recompute_topics|TopicDictionaryError' crates` returns zero matches.
- [ ] `crates/aise/src/domain/asset/mod.rs` and `crates/aise/src/context/mod.rs` contain no
      `entity` or `text_matcher` declaration or re-export.
- [ ] `cargo test -p aise --test dependency_direction_tests` passes.

### 5.9 Contract Alignment (W9)

- [ ] `TurnExecutionContext::replace_activation(&mut self, PreparedActivation) -> Result<(), TurnExecutionError>`
      matches §3.9 and its `retrieval_pipeline.rs` caller propagates the error with `?`.
- [ ] A phase test asserts `replace_activation` succeeds during `TurnPhase::Planned` and rejects
      calls outside that phase with `TurnExecutionError`.
- [ ] `ActivationRuleVersion` derives `PartialOrd` and `Ord`, has a private field, and exposes
      `from_rule` and `as_digest`.
- [ ] `FactSeed.retrieval_hint` and `RumorSeed.retrieval_hint` are `Option<RetrievalHint>` and
      the World Book fixtures round-trip.

### 5.10 Tests and Docs (W10, W11)

- [ ] `crates/aise/src/domain/knowledge/activation/tests/` and
      `crates/aise/src/context/activation/tests/` exist with the §3.1 files.
- [ ] `rg -n 'mod tests' crates/aise/src/domain/knowledge/activation crates/aise/src/context/activation`
      returns only `#[path]`-style declarations pointing at `tests/` files (`R-CODE-02`).
- [ ] `crates/aise/tests/context_preparation_retrieval_tests.rs` exists and covers Baseline
      activation, resume, deduplication, repair reuse, and owner-only Memory.
- [ ] `doc/design/2026-08-04-Architecture-gpt.md`,
      `doc/design/2026-08-06-StoryPackDesign-gpt.md`, and
      `doc/design/2026-08-08-context-preparation-retrieval-design-gpt.md` carry a header
      `> **Status**: Superseded by [World Info Entry Activation Refactor — Design](...)` and their
      Entity/Topic retrieval sections are marked superseded.
- [ ] `doc/exec/2026-08-08-context-preparation-retrieval-spec-gpt.md` carries the same superseded
      marker.

### 5.11 Regression and Toolchain

- [ ] All Phase 1 §5.1–§5.7 acceptance items that previously passed still pass.
- [ ] `cargo test -p aise --test world_info_activation_tests` passes with all 13 pre-existing
      engine scenarios unchanged.
- [ ] `cargo test -p aise --test knowledge_read_port_tests` passes.
- [ ] `cargo test -p aise --test context_preparation_retrieval_tests` passes.
- [ ] `cargo test -p aise --test prompt_context_contract_tests` passes.
- [ ] `cargo test -p aise --test asset_import_tests` passes.
- [ ] `cargo test -p aise --test dependency_direction_tests` passes.
- [ ] `cargo test -p aise --test persistence_tests` passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Phase 2 Preview API and author trace remain owned by
  [Phase 2](./2026-09-10-world-info-entry-activation-spec-phase-2-gpt.md). This spec only keeps
  `preview.rs` / `preview_service.rs` compiling against the changed engine and coordinator
  signatures; a separate Phase 2 conformance review is required.
- A concrete vector or BM25 `ActivationSeedProvider` requires its own Source Design and Spec.
- Role-owned Memory requires its own Source Design and Spec and will delete the owner-query
  transition retained in Phase 1.
- Cross-process or persisted activation index caching requires its own design; this spec keeps
  all caches process-local and rebuildable.

---

## 7. References

- Source design: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
- Phase 1 baseline: [Hard Cutover and Activation Runtime](./2026-09-10-world-info-entry-activation-spec-phase-1-gpt.md)
- Phase 2: [Activation Preview Tooling](./2026-09-10-world-info-entry-activation-spec-phase-2-gpt.md)
- Current coordinator (eager body prefetch): `crates/aise/src/context/activation/coordinator.rs:95-149`
- Current engine and errors: `crates/aise/src/domain/knowledge/activation/engine.rs:865`, `:1478-1503`
- Current index snapshot: `crates/aise/src/domain/knowledge/activation/contracts.rs:46`, `:346`
- Current SQLite index reader: `crates/aise/src/persistence/sqlite_activation.rs:35`
- Current rule validation: `crates/aise/src/domain/knowledge/activation/rule.rs:225`
- Unvalidated seed promotion: `crates/aise/src/story/instance_factory.rs:468`, `:510`
- Misplaced cache key: `crates/aise/src/config/activation.rs:245-254`
- Legacy residue: `crates/aise/src/domain/asset/entity.rs`, `crates/aise/src/domain/asset/text_matcher.rs`, `crates/aise/src/domain/asset/ids.rs:159`
- Legacy context errors: `crates/aise/src/context/error.rs:11`, `:17`, `:40`, `:42`
- Guardrails: `AGENTS.md` and `doc/agents/guardrails/`
