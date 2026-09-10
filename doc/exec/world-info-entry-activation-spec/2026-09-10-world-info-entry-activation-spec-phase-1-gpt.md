# World Info Entry Activation — Phase 1 Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-10
> **Status**: Proposed
> **Source Design**: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
> **Phase**: Phase 1 — hard cutover and activation runtime

---

## 1. Goal

Replace Entity/Topic knowledge retrieval with one deterministic, bounded, entry-owned activation engine used before and after planning, without retaining a legacy retrieval path.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Upgrade the World Book contract so every Fact and Rumor owns its activation rule.
- Delete Topic Dictionary, `KnowledgeEntity`, Entity/Topic signals, Entity/Topic retrievers, junction tables, and all related configuration.
- Add literal and regular-expression matching, secondary logic, bounded macros, constants, recursion, minimum-depth expansion, inclusion groups, deterministic probability, generation scope, sticky/cooldown/delay state, and hard budgets.
- Add immutable Frozen Pack plus Story overlay activation indexes and bounded fragment-match caching.
- Run automatic activation in `BaselineContextBuilder`, store one bounded continuation in `TurnExecutionContext`, and resume it in `ContextRetrievalPipeline`.
- Keep Planner retrieval output limited to exact indexed targets, delivery, and reason.
- Keep Fact and Rumor semantically separate and preserve existing audience authorization.
- Remove Memory Entity/Topic metadata and retrieve Memory only by exact owner `RoleId`.
- Add bounded literal activation terms for runtime-created Fact and Rumor entries.
- Commit dynamic activation rules, overlay version changes, timed-state deltas, Story text, Narrative resolution, and knowledge changes atomically.
- Rewrite repository assets, prompts, tests, migrations, and active documentation to the final contract.
- Provide the side-effect-free core execution mode required by the Phase 2 Preview API; Phase 1 does not expose an HTTP route.

### 2.2 Non-Goals

- Does not implement an embedding model, vector index, BM25 index, or concrete `ActivationSeedProvider`.
- Does not add provider/model/threshold/top-k fields to Story Pack, World Book, Planner output, or Turn requests.
- Does not move Memory into the Role aggregate; it retains the temporary owner-query persistence path.
- Does not add SillyTavern JSON import, Prompt position, message role, Author's Note, outlet, decorator, or Lorebook merge compatibility.
- Does not add a World Info editor or Preview HTTP/UI surface; Phase 2 owns author tooling.
- Does not change fixed Pipeline order, `TurnRuntime` orchestration, validation/repair budgets, or `TurnExecutionPipeline`.
- Does not let Fact probability represent truth or promote a Rumor to a Fact.
- Does not persist `ActivationContinuation`, fragment-match cache entries, compiled regex programs, or final activation decisions as Story authority.

### 2.3 Implementation Constraints

- This is one atomic hard refactor under `R-REFACTOR-01/02`. Phase 1 code must contain no fallback, compatibility parser, feature flag, adapter, dual schema, dual write, or inactive legacy branch.
- Delete every superseded type, module, field, SQL object, config key, test, fixture, Prompt field, and documentation contract in the same change.
- `TurnRuntime` remains the only Pipeline orchestrator. Pipelines communicate only through `&mut TurnExecutionContext`.
- Domain activation types must not import context, persistence, runtime, API, or adapter modules.
- `KnowledgeActivationEngine` is synchronous and pure over bounded inputs. SQLite access, cache access, and body loading belong to `KnowledgeActivationCoordinator`.
- Every cache, collection, fragment, pattern set, round, candidate set, evidence set, body read, and token budget has a positive trusted limit.
- No lock or database transaction may cross `.await`; no event, channel send, trace write, or I/O may occur while holding a write lock.
- Imported assets, dynamic LLM output, Story text, and activation Entry content are untrusted data and cannot select Prompt roles or trusted instructions.
- Code must follow `AGENTS.md`: index-only `mod.rs`/`lib.rs`, directory modules, dedicated test files, no ordinary code comments, compact imports, `forbid(unsafe_code)`, format and Clippy clean.
- Adding the workspace `regex` crate is permitted only with default features disabled and Unicode/performance features explicitly selected; it is the required linear-time regex engine and must be pinned consistently with the workspace MSRV.

### 2.4 Required Implementation Order

1. Add final activation Domain types, validation, configuration, and error contracts.
2. Upgrade World Book and runtime knowledge models; rewrite all Pack fixtures.
3. Add migration `0023_world_info_entry_activation.sql` and final persistence ports.
4. Add Frozen/overlay indexes, matcher, fragment cache, and pure state machine.
5. Integrate pre-Planner activation and one Turn-scoped continuation.
6. Replace Planner and post-Planner retrieval with exact external activation plus owner-only Memory reads.
7. Add dynamic Fact/Rumor literal rules and atomic overlay/timed-state commit.
8. Delete all Entity/Topic code and update prompts, tests, docs, and composition wiring.
9. Run all zero-match, migration, dependency, formatting, lint, and workspace tests before merge.

---

## 3. Contracts

### 3.1 File and Module Layout

```text
crates/aise/src/
├── domain/
│   ├── knowledge/
│   │   ├── activation/
│   │   │   ├── mod.rs
│   │   │   ├── rule.rs
│   │   │   ├── state.rs
│   │   │   ├── evidence.rs
│   │   │   └── tests/
│   │   ├── entry.rs
│   │   ├── fact.rs
│   │   ├── memory.rs
│   │   ├── query.rs
│   │   └── rumor.rs
│   ├── asset/
│   │   ├── world_book.rs
│   │   └── validation.rs
│   ├── story_instance/
│   │   └── snapshot.rs
│   └── turn/
│       ├── baseline.rs
│       ├── planning.rs
│       └── retrieval.rs
├── context/
│   ├── activation/
│   │   ├── mod.rs
│   │   ├── coordinator.rs
│   │   ├── engine.rs
│   │   ├── fragment_cache.rs
│   │   ├── index.rs
│   │   ├── matcher.rs
│   │   ├── scan_buffer.rs
│   │   └── tests/
│   ├── baseline_ctx_builder.rs
│   └── retrieval_pipeline.rs
├── persistence/
│   ├── activation_index_port.rs
│   ├── knowledge_read_port.rs
│   ├── sqlite_activation_index.rs
│   ├── sqlite_knowledge_reader.rs
│   ├── sqlite_snapshot.rs
│   └── sqlite_store.rs
├── planning/
│   ├── planner_output.rs
│   ├── retrieval_plan_builder.rs
│   └── writer_planner.rs
├── story/
│   ├── instance_factory.rs
│   └── story_state_extractor_prompt.rs
├── turn/
│   ├── turn_context.rs
│   └── turn_validation.rs
└── config/
    ├── activation.rs
    ├── assets.rs
    ├── context.rs
    └── retrieval.rs

crates/aise/assets/persistence/mig/
└── 0023_world_info_entry_activation.sql
```

Required deletions:

```text
crates/aise/src/domain/asset/entity.rs
crates/aise/src/context/candidate_retriever.rs
crates/aise/src/context/entity_candidate_retriever.rs
crates/aise/src/context/topic_candidate_retriever.rs
crates/aise/src/context/retrieval_signal_builder.rs
crates/aise/src/context/tests/retrieval_signal_builder_tests.rs
crates/aise/src/context/tests/entity_candidate_retriever_tests.rs
crates/aise/src/context/tests/topic_candidate_retriever_tests.rs
```

### 3.2 World Book and Activation Rule

`WorldBook` accepts only `(aise_world_v5, 5.0)`. Add `WorldSpec::V5` serialized as `aise_world_v5` and `AssetSpecVersion::V5_0` serialized as `5.0`; retain older shared `AssetSpecVersion` variants only for other asset families that still use them. World Book v4 is rejected before typed import.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBook {
    pub spec: WorldSpec,
    pub spec_version: AssetSpecVersion,
    pub world_book_key: WorldBookKey,
    pub meta: WorldBookMeta,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    pub retrieval_hint: Option<RetrievalHint>,
    pub salience: u8,
    pub activation: KnowledgeActivationRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    pub retrieval_hint: Option<RetrievalHint>,
    pub salience: u8,
    pub activation: KnowledgeActivationRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: BoundedText,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeActivationRule {
    #[serde(rename = "match")]
    pub match_rule: ActivationMatchRule,
    pub mode: ActivationMode,
    pub recursion: ActivationRecursionRule,
    pub selection: ActivationSelectionRule,
    pub timing: ActivationTimingRule,
    pub scope: ActivationScopeRule,
    pub budget_class: ActivationBudgetClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationMatchRule {
    #[serde(default)]
    pub keys: Vec<ActivationPattern>,
    #[serde(default)]
    pub secondary_keys: Vec<ActivationPattern>,
    pub secondary_logic: SecondaryLogic,
    pub case_sensitive: bool,
    pub match_whole_words: bool,
    pub scan_depth: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActivationPattern {
    Literal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecondaryLogic {
    AndAny,
    AndAll,
    NotAny,
    NotAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationMode {
    pub enabled: bool,
    pub constant: bool,
    pub exact_target_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRecursionRule {
    pub exclude_recursion: bool,
    pub prevent_recursion: bool,
    pub delay_until_recursion: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationSelectionRule {
    pub order: i32,
    pub probability: u8,
    #[serde(default)]
    pub groups: Vec<ActivationGroupKey>,
    pub group_override: bool,
    pub group_weight: u32,
    pub use_group_scoring: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationTimingRule {
    pub sticky_turns: u16,
    pub cooldown_turns: u16,
    pub delay_turns: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationScopeRule {
    #[serde(default)]
    pub generation_triggers: Vec<GenerationTrigger>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationTrigger {
    Normal,
    Continue,
    Regenerate,
    Repair,
    DryRunPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationBudgetClass {
    Normal,
    Reserved,
    Mandatory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationGroupKey(Arc<str>);

impl ActivationGroupKey {
    pub const MAX_BYTES: usize = 128;

    pub fn try_new(value: impl Into<String>) -> Result<Self, ActivationRuleValidationError>;
    pub fn as_str(&self) -> &str;
}
```

`ActivationPattern` serializes as a string. A string matching `/pattern/flags` is compiled as regex during validation; every other string is literal. The typed representation may store a private compiled classification after validation, but canonical asset JSON remains string-valued.

`ActivationGroupKey` accepts only lowercase ASCII values matching `[a-z0-9]+(?:[._-][a-z0-9]+)*`, rejects values longer than 128 UTF-8 bytes, and is bounded per Entry by trusted asset configuration. Group keys are activation controls only.

Defaults are exact:

- `secondary_logic = and_any`
- `case_sensitive = false`
- `match_whole_words = false`
- `enabled = true`
- `constant = false`
- `exact_target_only = false`
- all recursion booleans are `false`
- `delay_until_recursion = null`
- `order = 0`
- `probability = 100`
- `groups = []`
- `group_override = false`
- `group_weight = 100`
- `use_group_scoring = false`
- all timing counts are `0`
- `generation_triggers = []`, meaning all supported trigger types
- `budget_class = normal`

An enabled Entry is valid without a primary key only when `constant` or `exact_target_only` is true. `constant` and `exact_target_only` are mutually exclusive.

`Proposition.subject` and `Claim.subject` are trim-non-empty and bounded by `assets.max_text_bytes`.

Delete the generic Entity model from Narrative effects by adding this Narrative-owned type:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeParticipant {
    Role(RoleId),
    Location(LocationKey),
}
```

`WorldEventIntentDefinition.participants`, `WorldEventIntent.participants`, and their Prompt projections use `Vec<NarrativeParticipant>`. World, Scene, Narrative Node, and Event identities are not valid participant variants; their existing dedicated fields remain authoritative.

### 3.3 Pattern and Macro Contract

Literal normalization is:

```rust
pub fn normalize_activation_literal(value: &str, case_sensitive: bool) -> String;
```

It trims both ends, collapses every non-empty Unicode whitespace run to one ASCII space, preserves punctuation, and applies deterministic Unicode lowercase when `case_sensitive` is false. Matching never crosses fragment boundaries.

Whole-word matching uses Unicode alphanumeric boundaries. A match is valid when the preceding and following scalar, when present, are not Unicode alphanumeric and not `_`. Whole-word matching is disabled by default.

Regex syntax is `/pattern/flags`. Supported flags are `i`, `m`, `s`, and `u`; `u` is accepted as an explicit no-op because Unicode mode is always enabled. Duplicate or unknown flags fail validation. Regex matching does not inherit literal `case_sensitive` or whole-word behavior.

Only these literal macros are valid:

```text
{{player_name}}
{{player_role_label}}
```

Unknown macros fail asset or dynamic-rule validation. Macros are forbidden in regex. Expansion is escaped as literal data and is bounded by `max_macro_value_bytes` and `max_macro_expansion_bytes`.

### 3.4 Runtime Knowledge Models

```rust
pub struct WorldFact {
    pub id: FactId,
    pub key: Option<FactKey>,
    pub text: BoundedText,
    pub proposition: Option<Proposition>,
    pub retrieval_hint: RetrievalHint,
    pub activation: KnowledgeActivationRule,
    pub activation_rule_version: ActivationRuleVersion,
    pub salience: u8,
    pub source: KnowledgeSource,
}

pub struct SharedRumor {
    pub id: RumorId,
    pub key: Option<RumorKey>,
    pub content: BoundedText,
    pub claim: Option<Claim>,
    pub retrieval_hint: RetrievalHint,
    pub activation: KnowledgeActivationRule,
    pub activation_rule_version: ActivationRuleVersion,
    pub salience: u8,
    pub source_role_id: Option<RoleId>,
    pub truth_value: TruthValue,
    pub source: KnowledgeSource,
}

pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: RoleId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationRuleVersion(Sha256Digest);
```

`ActivationRuleVersion` is the SHA-256 digest of canonical serialized `KnowledgeActivationRule`. Fact/Rumor key remains identity only and never becomes an implicit activation pattern.

Delete `KnowledgeEntry::entities`, `KnowledgeEntry::topics`, `KnowledgeIndexMatch`, and every Entity/Topic field from Fact, Rumor, and Memory.

### 3.5 Scan Buffer

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFragmentKind {
    PlayerContribution,
    RecentStory,
    StorySummary,
    PlayerRoleName,
    PlayerRoleLabel,
    NarrativeDirection,
    NarrativeEvent,
    RecursionContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScanFragmentId(Sha256Digest);

#[derive(Debug, Clone)]
pub struct ScanFragment {
    pub id: ScanFragmentId,
    pub kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_order: u32,
    pub content_hash: Sha256Digest,
    pub text: BoundedText,
}

#[derive(Debug, Clone)]
pub struct ActivationScanBuffer {
    fragments: Vec<ScanFragment>,
}

impl ActivationScanBuffer {
    pub fn try_new(
        fragments: Vec<ScanFragment>,
        limits: ActivationScanLimits,
    ) -> Result<Self, ActivationError>;

    pub fn fragments(&self) -> &[ScanFragment];
    pub fn visible_at_depth(&self, depth: u16) -> impl Iterator<Item = &ScanFragment>;
}
```

Fragment order is Player Contribution, player display text, Narrative display text, Recent Story newest to oldest, then Summary. Summary has a dedicated depth after all configured Recent Story depths. Internal IDs and keys are never converted into fragment text.

`BaselineContextBuilder` runs `NarrativeProjector` once before activation and stores that exact projection in `TurnExecutionContext`. `WriterPlanner` must reuse it and must not project again.

### 3.6 Activation Index and Fragment Cache

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationIndexSnapshotRef {
    pub story_id: StoryId,
    pub pack_digest: Sha256Digest,
    pub base_revision: StoryRevision,
    pub overlay_version: u64,
    pub matcher_version: u32,
}

pub struct ActivationIndexSnapshot {
    pub reference: ActivationIndexSnapshotRef,
    pub literal_index: FrozenLiteralIndex,
    pub regex_set: FrozenRegexSet,
    pub constant_entries: Vec<KnowledgeSourceId>,
    pub metadata: BTreeMap<KnowledgeSourceId, ActivationEntryMetadata>,
}

#[derive(Debug, Clone)]
pub struct ActivationEntryMetadata {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub rule: KnowledgeActivationRule,
    pub rule_version: ActivationRuleVersion,
    pub salience: u8,
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

#[async_trait]
pub trait ActivationIndexPort: Send + Sync {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        limits: ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexSnapshot>, StoreError>;
}
```

The Frozen Pack index is keyed by Pack digest. The Story overlay contains only runtime Fact/Rumor additions, updates, and tombstones. `load_snapshot` must return one immutable composition whose Story ID, Pack digest, and base revision match `KnowledgeSnapshotRef`.

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
pub struct FragmentMatchCacheValue {
    pub matches: Vec<FragmentPatternMatch>,
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

pub trait FragmentMatchCache: Send + Sync {
    fn get(&self, key: &FragmentMatchCacheKey) -> Option<Arc<FragmentMatchCacheValue>>;
    fn insert(
        &self,
        key: FragmentMatchCacheKey,
        value: FragmentMatchCacheValue,
    ) -> Result<(), ActivationError>;
}
```

The cache is a process-local bounded LRU owned by `KnowledgeActivationCoordinator`; Phase 1 adds no cache table or cross-process cache. It stores only pattern identity, source ID, primary/secondary kind, local evidence offsets, and match score. It must not store Entry content, final activation/rejection, group winner, probability result, budget decision, timed state, or continuation. Its owner enforces bounds for stories, fragments, matches, evidence bytes, and total estimated bytes.

Frozen Pack indexes are validated and warmed during Pack import. They remain derived artifacts keyed by Pack digest and may be rebuilt from canonical Pack Entries on cache miss. Overlay index metadata is rebuilt from canonical runtime Fact/Rumor rows plus tombstones at the committed `overlay_version`; no index artifact is authoritative.

### 3.7 Activation Inputs, Results, and Continuation

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationRunMode {
    CommitEligible,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationSeedKind {
    Constant,
    Sticky,
    TextMatch,
    PlannerExactTarget,
    Provider,
    PreviewOverride,
    Recursion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPatternKind {
    PrimaryLiteral,
    PrimaryRegex,
    SecondaryLiteral,
    SecondaryRegex,
    Constant,
    Sticky,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationRejectionReason {
    Disabled,
    ScopeMismatch,
    Delayed,
    Cooldown,
    RecursionExcluded,
    RecursionLevelLocked,
    SecondaryCondition,
    GroupLoser,
    Probability,
    Budget,
    Duplicate,
    WorkLimit,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationEvidence {
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub stable_fragment_order: u32,
    pub match_count: u16,
    pub group_score_contribution: u16,
    pub round: u16,
    pub recursion_level: u16,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ActivationWorkUsage {
    pub scan_fragments: usize,
    pub scan_bytes: usize,
    pub scan_tokens: u64,
    pub pattern_matches: usize,
    pub candidate_evaluations: usize,
    pub recursion_steps: u16,
    pub recursion_fragments: usize,
    pub recursion_bytes: usize,
    pub recursion_tokens: u64,
    pub activated_entries: usize,
    pub knowledge_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ExternalActivationSeed {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub kind: ActivationSeedKind,
    pub provider_rank: Option<u32>,
    pub mandatory: bool,
}

#[derive(Debug, Clone)]
pub struct ActivationRequest<'a> {
    pub story_id: &'a StoryId,
    pub turn_number: TurnNumber,
    pub generation_trigger: GenerationTrigger,
    pub mode: ActivationRunMode,
    pub knowledge_snapshot: &'a KnowledgeSnapshotRef,
    pub index_snapshot: &'a ActivationIndexSnapshot,
    pub scan_buffer: &'a ActivationScanBuffer,
    pub timed_state: &'a [ActivationTimedState],
    pub external_seeds: &'a [ExternalActivationSeed],
    pub continuation: Option<ActivationContinuation>,
    pub limits: ActivationRuntimeLimits,
}

#[derive(Debug, Clone)]
pub struct ActivationResult {
    pub activated: Vec<ActivatedKnowledgeRef>,
    pub continuation: ActivationContinuation,
    pub pending_timed_state: PendingActivationStateDelta,
    pub rejection_summary: BTreeMap<ActivationRejectionReason, u32>,
    pub stop_reason: ActivationStopReason,
}

#[derive(Debug, Clone)]
pub struct ActivatedKnowledgeRef {
    pub source_id: KnowledgeSourceId,
    pub deliveries: Vec<KnowledgeDelivery>,
    pub activation_class: ActivationSeedKind,
    pub rank: u32,
    pub token_cost: u64,
    pub evidence: Vec<ActivationEvidence>,
}

#[derive(Debug, Clone)]
pub struct ActivationContinuation {
    pub turn_number: TurnNumber,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
    pub index_snapshot: ActivationIndexSnapshotRef,
    pub activated: BTreeMap<KnowledgeSourceId, ActivatedKnowledgeRef>,
    pub terminal_rejections: BTreeMap<KnowledgeSourceId, ActivationRejectionReason>,
    pub failed_probability: BTreeSet<KnowledgeSourceId>,
    pub group_winners: BTreeMap<ActivationGroupKey, KnowledgeSourceId>,
    pub recursion_level: u16,
    pub consumed: ActivationWorkUsage,
    pub evidence_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationStopReason {
    Complete,
    MinimumSatisfied,
    MaximumDepthReached,
    RecursionExhausted,
    WorkTrimmed,
}
```

Continuation is valid only for the same logical Turn, knowledge snapshot, index snapshot, generation trigger, and configured hard limits. It is held only by `TurnExecutionContext`, is never serialized into Prompt data, and is dropped at Turn termination.

Activation identity and group selection are global to the logical Turn. Delivery is an authorized projection of one activated Source ID, not a second activation. Adding a later Character delivery for an already activated Rumor reuses the existing probability, group, evidence, and recursion result and charges only that delivery's context budget.

### 3.8 State Machine

```rust
pub struct KnowledgeActivationEngine;

impl KnowledgeActivationEngine {
    pub fn run(&self, request: ActivationRequest<'_>) -> Result<ActivationResult, ActivationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMachineState {
    Initial,
    Recursion,
    DepthExpansion,
    Resumed,
    Complete,
}
```

Each round performs this exact sequence:

1. Collect new candidates from visible fragment matches, constant/sticky sets, recursion matches, or external seeds.
2. Reject duplicate, disabled, scope-mismatched, delayed, cooldown, recursion-excluded, or recursion-level-locked candidates.
3. Evaluate primary and secondary logic for text candidates; forced, constant, and sticky candidates skip keyword conditions.
4. Resolve inclusion groups.
5. Evaluate deterministic probability once per Entry per Turn.
6. Sort by §3.10 and admit against work and knowledge budgets.
7. Return IDs for bounded body loading.
8. Add admitted body content as separate recursion fragments unless `prevent_recursion`.
9. Update continuation, evidence, counters, and pending timed-state delta.
10. Continue recursion, expand depth toward minimum activation, or complete.

The coordinator loads bodies only after admission, validates the returned kind and Source ID, supplies admitted content for the next recursion round, and removes an admitted candidate if body loading fails. Mandatory body-load failure aborts the Turn.

Initial matching exposes `initial_scan_depth`. If the total successfully activated Source IDs is below `minimum_activations`, the engine increases visible depth by one and re-evaluates only newly visible fragments until the minimum is reached, `max_scan_depth` is reached, or `max_depth_expansions` is consumed. Depth expansion never scans recursion fragments and failure to reach the minimum at maximum depth completes with `MaximumDepthReached`; it is not an invariant error.

### 3.9 Secondary, Group, Probability, and Timing Semantics

- Primary keys use OR.
- `and_any` requires at least one secondary match.
- `and_all` requires every distinct secondary pattern to match.
- `not_any` requires no secondary pattern to match.
- `not_all` requires at least one secondary pattern not to match.
- Negative secondary matches add no group score.
- Positive group score is the count of distinct matched primary plus positive secondary pattern identities.
- One logical Turn has at most one new winner per group across all deliveries.
- Winner order is active sticky, score descending, `group_override` then `order` descending, deterministic weighted choice, and Source ID ascending.

Probability uses an unsigned sample derived from SHA-256 over domain-separated canonical bytes:

```text
domain = "aise.knowledge.activation.probability.v1"
seed = story_id || turn_number || source_id || activation_rule_version
```

Group weighted selection uses domain `aise.knowledge.activation.group.v1`. It never reuses the probability sample. A probability of `0` always rejects and `100` always admits without sampling.

Timed-state contract:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationTimedState {
    pub source_id: KnowledgeSourceId,
    pub rule_version: ActivationRuleVersion,
    pub sticky_through_turn: Option<TurnNumber>,
    pub cooldown_through_turn: Option<TurnNumber>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingActivationStateDelta {
    pub upserts: Vec<ActivationTimedState>,
    pub deletes: Vec<KnowledgeSourceId>,
}
```

If Entry activation occurs at Turn `T`, `sticky_turns = N` makes it sticky for `T+1..=T+N`. Cooldown begins after the sticky range and suppresses the next `cooldown_turns` committed Turn numbers. `delay_turns = N` suppresses activation while current Turn number is `<= N`. A rule-version mismatch deletes old timed state in the pending delta and evaluates the Entry as having no timed state.

Preview computes the same pending delta but never persists it. Repair reuses the original activated knowledge and continuation and does not run activation again.

### 3.10 Ranking and Budgets

Activation-class order is:

1. active sticky
2. authorized external exact target with `mandatory = true`
3. mandatory constant
4. normal constant
5. literal/regex text match
6. provider candidate

Within one class:

1. `selection.order` descending
2. group/match score descending
3. scan source priority ascending
4. nearest recency depth ascending
5. `salience` descending
6. provider rank ascending, with missing rank last
7. `KnowledgeSourceId` ascending

`ActivationRuntimeLimits` and its nested config must cover at least:

```rust
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
```

`AiseConfig` adds `activation: ActivationConfig`. Rule limits are trusted application configuration, not Story Pack fields. Every maximum is positive. `minimum_activations = 0` is the sole zero-valued exception and disables depth expansion. `initial_scan_depth <= max_scan_depth`; `max_group_key_bytes <= ActivationGroupKey::MAX_BYTES`; an Entry `scan_depth` can reduce visibility but cannot exceed `max_scan_depth`. Actual depth expansion stops at the first of `max_depth_expansions` or `max_scan_depth`. Summary is visible initially only when configured as an initial source; otherwise it becomes visible only at maximum depth when `include_summary_at_max_depth` is true. `reserved_tokens <= max_total_tokens`, `mandatory_tokens <= max_total_tokens`, and per-audience limits do not exceed totals. `0` never means unlimited for a maximum.

Normal candidates stop at the soft normal allocation. Reserved candidates may consume reserved allocation. Mandatory candidates are admitted first but remain subject to hard item, single-entry, per-audience, and total token limits. Inability to admit mandatory content returns `ActivationError::MandatoryBudgetExceeded`.

### 3.11 Persistence Read Ports

Replace Entity/Topic reads with:

```rust
#[derive(Debug, Clone)]
pub struct SourceKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub source_ids: &'a [KnowledgeSourceId],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct OwnerMemoryQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub owner: &'a RoleId,
    pub limit: usize,
    pub max_item_bytes: usize,
}

#[async_trait]
pub trait KnowledgeReadPort: Send + Sync {
    async fn find_by_source_ids(
        &self,
        query: SourceKnowledgeQuery<'_>,
    ) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn find_memories_by_owner(
        &self,
        query: OwnerMemoryQuery<'_>,
    ) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn list_index(
        &self,
        query: KnowledgeIndexQuery<'_>,
    ) -> Result<Vec<KnowledgeIndexRecord>, StoreError>;
}
```

`find_by_entities`, `find_by_topics`, `EntityKnowledgeQuery`, `TopicKnowledgeQuery`, and `KnowledgeLookupHit` are deleted. `list_index` returns Fact/Rumor `source_id + retrieval_hint` only and excludes already supplied Source IDs at the Baseline projection boundary.

Committed timed state uses a separate read boundary:

```rust
#[derive(Debug, Clone)]
pub struct ActivationTimedStateQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub limit: usize,
}

#[async_trait]
pub trait ActivationTimedStateReadPort: Send + Sync {
    async fn load_timed_state(
        &self,
        query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<ActivationTimedState>, StoreError>;
}
```

### 3.12 Baseline, Planner, and Retrieval Integration

`BaselineContext` deletes `retrieval_signals`. It retains `RelevantWorldKnowledge` and `knowledge_index`; activated Fact/Rumor bodies enter the former, while non-provided indexed hints enter the latter.

```rust
pub struct PreparedActivation {
    pub continuation: ActivationContinuation,
    pub pending_timed_state: PendingActivationStateDelta,
}

pub struct TurnExecutionContext {
    activation: Option<PreparedActivation>,
}

impl TurnExecutionContext {
    pub fn activation(&self) -> Option<&PreparedActivation>;
    pub fn set_prepared_context(
        &mut self,
        snapshot: StoryReadSnapshot,
        baseline: BaselineContext,
        narrative_projection: NarrativeProjection,
        activation: PreparedActivation,
    ) -> Result<(), TurnExecutionError>;
    pub fn replace_activation(
        &mut self,
        activation: PreparedActivation,
    ) -> Result<(), TurnExecutionError>;
}
```

`BaselineContextBuilder` obtains one Story Snapshot, one matching Activation Index Snapshot, committed timed state, and one Narrative Projection. It builds the Scan Buffer, runs automatic activation, loads admitted Fact/Rumor bodies, builds Baseline and Knowledge Index, and stores all four prepared values in one phase transition.

Planner output remains exact-target based:

```rust
pub struct KnowledgeRetrievalRequest {
    pub delivery: KnowledgeDelivery,
    pub target_source_id: KnowledgeSourceId,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub mandatory: bool,
}
```

Planner DTOs may provide only indexed `target_id`, `role_id` for Character delivery, and `reason`. The server resolves `target_id` to `KnowledgeSourceId`. Unknown targets are rejected as invalid Planner output rather than logged and dropped. Planner output cannot contain keys, regex, provider, score, top-k, recursion, probability, group, or budget fields.

`ContextRetrievalPipeline` has no `CandidateRetriever` collection. It converts exact requests into authorized external seeds, resumes the existing continuation through the same coordinator, loads newly admitted Fact/Rumor bodies, reads Memory by owner for requested AI Roles, and builds `RetrievedContext`. A Source ID already present in Baseline is not repeated in the Writer partition.

Character delivery permits authorized Rumor and owner Memory only. For each requested AI Role, the Pipeline projects already activated Rumors into `known_rumors` only when existing visibility rules authorize that Role; this projection does not reactivate, resample, regroup, or recurse the Rumor. A Character exact Rumor target may add an authorized delivery to an already activated Source ID or activate a new Source ID through the shared continuation. Fact remains Writer-only. Memory never enters the activation engine.

### 3.13 Future Provider Boundary

Phase 1 defines but does not implement:

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

#[async_trait]
pub trait ActivationSeedProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    async fn candidates(
        &self,
        request: ActivationSeedRequest<'_>,
    ) -> Result<Vec<ProviderActivationCandidate>, ActivationProviderError>;
}
```

No concrete provider, empty provider, provider registry, config block, or composition-root registration is added in Phase 1.

### 3.14 Dynamic Fact and Rumor Rules

Extractor DTOs add:

```rust
pub struct FactDraftDto {
    pub content: String,
    pub retrieval_hint: String,
    pub activation_terms: Vec<String>,
    pub exact_target_only: bool,
}

pub struct RumorDraftDto {
    pub content: String,
    pub retrieval_hint: String,
    pub activation_terms: Vec<String>,
    pub exact_target_only: bool,
    pub source_role_id: String,
    pub truth_value: TruthValue,
}
```

Fact/Rumor update DTOs use `activation_terms: Option<Vec<String>>`; `None` preserves the existing rule, while `Some` replaces only literal primary keys and resets no other trusted rule field. Runtime output cannot create regex, macro, constant, group, probability, timing, recursion, generation-scope, or budget-class values.

New dynamic Fact/Rumor entries require non-empty `retrieval_hint` and either at least one valid literal term or `exact_target_only = true`. Validation rejects empty, duplicate-after-normalization, oversized, macro-bearing, regex-shaped, or over-count terms.

Delete `recompute_topics` and every call to it.

### 3.15 SQLite Migration and Atomic Commit

Add `0023_world_info_entry_activation.sql`. It must:

1. Fail with named constraint `world_info_activation_legacy_data_present` when an existing Story Pack, Story Instance, or knowledge row uses the old World Book contract under the repository's fresh-cutover policy.
2. Rebuild `story_packs` without `topic_dictionary_json`.
3. Rebuild `knowledge_entries` with `activation_rule_json` and `activation_rule_version` required for Fact/Rumor and null for Memory.
4. Drop `knowledge_entry_entities` and `knowledge_entry_topics`.
5. Add `activation_overlay_version INTEGER NOT NULL DEFAULT 0` to Story-scoped authority.
6. Create:

```sql
CREATE TABLE knowledge_activation_timed_state (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    rule_version TEXT NOT NULL,
    sticky_through_turn INTEGER,
    cooldown_through_turn INTEGER,
    PRIMARY KEY (story_id, source_id),
    CHECK (sticky_through_turn IS NULL OR sticky_through_turn > 0),
    CHECK (cooldown_through_turn IS NULL OR cooldown_through_turn > 0)
);
```

7. Run `PRAGMA foreign_key_check`.

`TurnCommitSpec` adds:

```rust
pub activation_state_delta: PendingActivationStateDelta,
```

Within one SQLite transaction, commit Story text, Narrative resolution, knowledge mutations, canonical activation rules, overlay-version increment/tombstones, timed-state delta, high-water values, outbox, idempotency result, and LLM ledger. Any failure rolls back all of them. Cache refresh occurs only after commit and may be skipped because a versioned cache miss is semantically complete.

### 3.16 Errors

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
```

Ordinary candidate count/token exhaustion produces stable trimming plus a rejection reason. Snapshot, continuation, rule, timed-state, recursion-step, mandatory-budget, and store invariant failures abort the Turn.

Turn error codes are:

```text
activation_rule_invalid
activation_regex_invalid
activation_index_mismatch
activation_snapshot_conflict
activation_continuation_mismatch
activation_work_limit
activation_recursion_limit
activation_mandatory_budget
activation_target_unauthorized
activation_timed_state_invalid
activation_provider_failed
```

### 3.17 Observability

Emit bounded structured spans:

```text
knowledge.activation.prepare
knowledge.activation.round
knowledge.activation.resume
knowledge.activation.provider
knowledge.activation.commit
```

Fields include Story ID, Turn number, base revision, Pack digest, overlay version, state/round/recursion level, fragment and scan byte counts, literal/regex match counts, candidate/activated/rejected counts, rejection-reason counts, provider candidate count, consumed work/token budget, status, and error code.

Production telemetry must not record Story text, Summary text, Player Contribution, pattern text, regex text, Entry content, Memory content, Prompt content, or raw LLM output. Source IDs and bounded evidence details are permitted only under the existing development redaction policy.

---

## 4. Behavior Rules

1. **WIA1-01 — Entry Authority**: Fact/Rumor activation behavior comes only from its canonical `KnowledgeActivationRule`; stable keys and retrieval hints never trigger automatic activation.
2. **WIA1-02 — Hard Removal**: No Topic Dictionary, `TopicKey`, `KnowledgeEntity`, Entity/Topic signal, matcher, retriever, query, index table, or compatibility parser remains.
3. **WIA1-03 — One Turn State**: Pre-Planner and post-Planner activation use one continuation and one cumulative budget.
4. **WIA1-04 — Snapshot Alignment**: Story, Knowledge, and Activation Index snapshots share Story ID, Pack digest, and base revision.
5. **WIA1-05 — Fragment Isolation**: A literal or regex match cannot span two Scan Fragments.
6. **WIA1-06 — Derived Text Allowlist**: Only fragment kinds in §3.5 may enter the Scan Buffer; internal keys and bulk Domain catalogs cannot.
7. **WIA1-07 — Match Completeness**: All distinct matching patterns are retained up to evidence bounds before secondary and group scoring.
8. **WIA1-08 — Regex Safety**: Regex compiles during validation with the bounded linear-time engine; runtime never falls back to literal.
9. **WIA1-09 — Macro Safety**: Only §3.3 literal macros are accepted; expansion cannot alter regex structure.
10. **WIA1-10 — One Activation**: One `KnowledgeSourceId` activates at most once per logical Turn; one activation may have multiple authorized deliveries.
11. **WIA1-11 — Terminal Rejection**: Probability, group, scope, and budget rejection is terminal under unchanged conditions and cannot loop.
12. **WIA1-12 — Recursion Source**: Only admitted Entry bodies become separate recursion fragments.
13. **WIA1-13 — Recursion Controls**: `prevent_recursion`, `exclude_recursion`, and positive `delay_until_recursion` follow §3.8 exactly.
14. **WIA1-14 — Hard Stops**: Every work, step, item, byte, and token bound is positive; `0 = unlimited` is invalid.
15. **WIA1-15 — Determinism**: Input permutations, retry, repair, resume, and Preview produce the same ordered decisions for the same identities and config.
16. **WIA1-16 — Group Isolation**: Groups control mutual exclusion only and never become retrieval keys, Prompt categories, or semantic labels.
17. **WIA1-17 — Fact Truth**: Probability controls context inclusion only and never modifies Fact truth.
18. **WIA1-18 — External Safety**: External seeds skip key matching only; they still pass authorization, group, probability policy, duplicate suppression, recursion policy, and hard budget.
19. **WIA1-19 — Sticky Limits**: Sticky bypasses keys, cooldown, probability, and recursion exclusion, but not disabled, scope, authorization, group winner, or hard budget.
20. **WIA1-20 — Commit Timing**: Timed state and overlay versions advance only in a successful Turn commit.
21. **WIA1-21 — Repair Reuse**: Story repair and state re-extraction reuse activation output and never rescan or resample.
22. **WIA1-22 — Cache Semantics**: Fragment cache hit/miss changes performance only; final activation output is byte-equivalent.
23. **WIA1-23 — Cache Invalidation**: Content, Pack, overlay, matcher, or macro digest changes prevent stale cache reuse.
24. **WIA1-24 — Body Loading**: The hot path never loads all Fact/Rumor bodies; body reads occur only for admitted IDs and bounded Knowledge Index projection.
25. **WIA1-25 — Audience**: Fact is Writer-only; Rumor follows existing visibility; Character Memory is owner-only.
26. **WIA1-26 — Memory Boundary**: Memory has no activation rule, key, regex, group, probability, timing, Entity, or Topic metadata.
27. **WIA1-27 — Dynamic Safety**: LLM-created activation data is limited to validated literal primary terms or exact-target-only.
28. **WIA1-28 — Prompt Boundary**: Patterns, rules, evidence, group keys, provider metadata, and continuation are never serialized into model context.
29. **WIA1-29 — Pipeline Isolation**: Activation integration adds no Pipeline and no Pipeline-to-Pipeline call.
30. **WIA1-30 — Provider Absence**: Phase 1 contains the provider trait only; no provider implementation, registration, endpoint, model, or config exists.

### 4.1 Error Handling

- Invalid assets and dynamic rules fail before persistence with stable validation paths.
- Invalid regex never reaches a Turn.
- Index, snapshot, or continuation mismatch aborts before returning knowledge.
- Ordinary non-mandatory overflow records `work_limit` or `budget` rejection and trims deterministically only where §3.16 permits.
- Mandatory overflow aborts before Story generation.
- Missing or unauthorized exact targets return typed errors; they are not warned and dropped.
- Store serialization failures never skip malformed rows.
- External/asset/LLM input paths contain no `unwrap`, `expect`, or panic.

### 4.2 Concurrency

- Index and timed-state reads use short transactions closed before matching or any LLM call.
- Matcher, secondary evaluation, group selection, probability, ranking, and trimming are synchronous bounded work.
- Coordinator body reads are bounded and sequential unless a fixed-size batched `find_by_source_ids` query is used.
- Fragment cache uses bounded ownership and never exposes a write guard across I/O or trace emission.
- No per-Entry task, detached future, unbounded channel, hidden queue, or unbounded `join_all` is added.
- Any future embedding call must pass through the shared LLM concurrency limiter; Phase 1 makes no embedding call.

### 4.3 Observability

- Every activation execution has one prepare span and one span per bounded round.
- Resume uses `knowledge.activation.resume`, not a second prepare span.
- Commit records timed upsert/delete count and overlay-version change without recording content.
- Zero-match, scope rejection, group rejection, probability rejection, budget trimming, depth exhaustion, and recursion stop are distinguishable by structured counts.

---

## 5. Acceptance Criteria

### 5.1 Asset and Domain

- [ ] World Book round-trips the §3.2 shape and rejects `topics`, Entry `entities`, and Entry `topics`.
- [ ] All four secondary logic variants have positive and negative tests.
- [ ] Literal tests cover Unicode case normalization, whitespace collapse, Chinese substring matching, whole-word boundaries, punctuation, and fragment isolation.
- [ ] Regex tests cover all supported flags, unsupported/duplicate flags, invalid expressions, count/length/program limits, and no literal fallback.
- [ ] Macro tests cover both allowed names, unknown names, regex rejection, escaping, and expansion byte limits.
- [ ] `ActivationRuleVersion` changes on every canonical rule change and not on content-only change.
- [ ] Dynamic Fact/Rumor validation accepts literal terms/exact-target-only and rejects every advanced policy field.

### 5.2 Engine and Determinism

- [ ] Engine tests cover constant, primary OR, secondary logic, recursion chain, cycle, `prevent_recursion`, `exclude_recursion`, recursion delay, and depth expansion.
- [ ] Group tests cover sticky precedence, score, override/order, deterministic weight, multi-group conflict, and stable Source-ID tie.
- [ ] Probability tests prove 0/100 boundaries, stable replay, independent group domain, retry/repair stability, and changed Turn behavior.
- [ ] Timing tests prove exact sticky/cooldown/delay Turn ranges and rule-version invalidation.
- [ ] Budget tests cover normal/reserved/mandatory classes, per-audience and total limits, single-entry overflow, deterministic trimming, and mandatory failure.
- [ ] One Source ID activates at most once and one failed probability is sampled at most once in Initial + recursion + resume.
- [ ] Input permutation tests produce byte-identical activated IDs, ranks, evidence summaries, rejections, and pending delta.

### 5.3 Cache and Snapshot

- [ ] Cache hit and miss produce byte-identical `ActivationResult`.
- [ ] Changing fragment hash, Pack digest, overlay version, matcher version, or macro digest causes a miss.
- [ ] `and_all`, `not_any`, and `not_all` evaluate after merging all visible fragment matches.
- [ ] Summary replacement uses a new content hash and never inherits removed Segment Entry IDs.
- [ ] Cache limits evict deterministically or by bounded LRU and never exceed configured story/fragment/match/evidence/byte bounds.
- [ ] Activation Index Snapshot mismatch returns no knowledge body.

### 5.4 Pipeline, Planner, and Prompt

- [ ] Baseline executes activation once before Planner and stores one continuation.
- [ ] WriterPlanner reuses the Baseline Narrative Projection and does not call `NarrativeProjector` again.
- [ ] Context Retrieval resumes the same continuation and cumulative budget for exact targets.
- [ ] Baseline and Retrieved Context deduplicate the same Source ID.
- [ ] Repair and state re-extraction make zero activation/index/cache/probability calls.
- [ ] Planner schema rejects keys, regex, providers, scores, top-k, recursion, probability, groups, and budgets.
- [ ] Unknown and unauthorized exact targets fail with stable codes.
- [ ] Prompt snapshots contain Fact/Rumor bodies only and exclude activation rules, evidence, group keys, continuation, and timed state.
- [ ] Character tests prove Fact exclusion, Rumor visibility, and owner-only Memory.

### 5.5 Persistence and Atomicity

- [ ] Migration `0023` succeeds on a fresh database after `0022` and `PRAGMA foreign_key_check` is empty.
- [ ] The configured destructive-upgrade fixture fails with `world_info_activation_legacy_data_present` without modifying rows.
- [ ] `knowledge_entries` stores canonical rules for Fact/Rumor and no rule for Memory.
- [ ] `knowledge_entry_entities`, `knowledge_entry_topics`, and `story_packs.topic_dictionary_json` do not exist after migration.
- [ ] Story commit rollback leaves Story text, knowledge rows, overlay version, timed state, Narrative state, outbox, and ledger unchanged.
- [ ] Successful dynamic Fact/Rumor commit increments overlay version and is visible starting with the next Turn.
- [ ] Failed Turn, Preview, repair, and dry-run leave timed state and overlay authority unchanged.

### 5.6 Static Removal

- [ ] `rg -n '\bTopicKey\b|\bTopicDefinition\b|\bKnowledgeEntity\b|\bEntitySignal\b|\bTopicSignal\b|\bRetrievalSignals\b|\bKnowledgeIndexMatch\b' crates examples config` returns zero matches.
- [ ] `rg -n 'EntityCandidateRetriever|TopicCandidateRetriever|EntityKnowledgeQuery|TopicKnowledgeQuery|find_by_entities|find_by_topics' crates` returns zero matches.
- [ ] `rg -n 'knowledge_entry_entities|knowledge_entry_topics|topic_dictionary_json' crates/aise/src crates/aise/assets/persistence/mig/0023_world_info_entry_activation.sql` returns only migration drop/assert statements.
- [ ] `rg -n 'max_topics|max_topic_aliases_per_topic|max_entities_per_entry|max_topics_per_entry|max_entity_catalog|max_signal_entities|max_signal_topics|max_candidate_retrievers' crates/aise/src/config config` returns zero matches.
- [ ] Deleted modules from §3.1 do not exist.
- [ ] `examples/snake_pack.json`, `examples/demo_pack.json`, and all fixtures use Entry activation rules.
- [ ] Active architecture/context docs mark Entity/Topic retrieval contracts as superseded.

### 5.7 Toolchain

- [ ] `cargo test -p aise --test world_info_activation_tests` passes.
- [ ] `cargo test -p aise --test knowledge_read_port_tests` passes.
- [ ] `cargo test -p aise --test context_preparation_retrieval_tests` passes.
- [ ] `cargo test -p aise --test prompt_context_contract_tests` passes.
- [ ] `cargo test -p aise --test dependency_direction_tests` passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Activation Preview HTTP and author diagnostics are implemented by [Phase 2](./2026-09-10-world-info-entry-activation-spec-phase-2-gpt.md).
- A vector activation provider requires its own Source Design and Spec; it may implement `ActivationSeedProvider` without changing the state machine.
- Role-owned Memory requires its own Source Design and Spec and will delete the owner-query transition retained here.
- New generation trigger routes must map to the closed enum through a separate runtime/API contract.

---

## 7. References

- Source design: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
- Phase 2: [Activation Preview Tooling](./2026-09-10-world-info-entry-activation-spec-phase-2-gpt.md)
- Superseded retrieval spec: [Context Preparation and Retrieval](../2026-08-08-context-preparation-retrieval-spec-gpt.md)
- Current World Book: `crates/aise/src/domain/asset/world_book.rs`
- Current Baseline retrieval: `crates/aise/src/context/baseline_ctx_builder.rs`
- Current post-Planner retrieval: `crates/aise/src/context/retrieval_pipeline.rs`
- Current knowledge port: `crates/aise/src/persistence/knowledge_read_port.rs`
- Current dynamic knowledge enrichment: `crates/aise/src/domain/turn/extraction.rs`
- Guardrails: `AGENTS.md` and `doc/agents/guardrails/`
