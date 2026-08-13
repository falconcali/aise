# Context Preparation and Retrieval — Spec

> **Model**: GPT-5.6 Codex
> **Date**: 2026-08-08
> **Status**: Proposed
> **Source Design**: [Context Preparation and Retrieval — Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
> **Phase**: Phases 1–3

---

## 1. Goal

Replace the legacy Turn context path with a bounded Story Pack v3 baseline, deterministic Entity/Topic retrieval, audience-isolated retrieved context, and fixed-profile typed LLM contexts.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Make `domain::story_instance::snapshot::StoryReadSnapshot` the only Turn snapshot and delete the legacy `domain::story_state::StoryReadSnapshot` path.
- Add stable story sequencing, Summary coverage, and a validated `StoryContinuity` contract with no overlap or gaps.
- Replace the legacy `BaselineContext` fields with `StoryProfile`, `InstanceSettings`, resolved character views, structured active constraints, continuity, Narrative state, and deterministic retrieval signals.
- Keep Player Input authoritative in `TurnRequest`; stage contexts may borrow or clone it, but Snapshot, Baseline, and `WriterPlan` must not own a second copy.
- Add a validated World Book `TopicDictionary` and preserve Entity, Topic, salience, source ID, source revision, and knowledge kind metadata on runtime Fact, Rumor, and Memory entries.
- Add `KnowledgeSnapshotRef` and a revision-scoped `KnowledgeReadPort` that performs indexed Entity/Topic reads without holding a database transaction across LLM calls.
- Implement `CandidateRetriever`, `EntityCandidateRetriever`, and `TopicCandidateRetriever`; reserve the same trait for future BM25 and Embedding providers without adding fake implementations.
- Make `WriterPlanner` evaluate `NarrativeDirector` before its LLM call, parse strict semantic context gaps, and merge Automatic, Narrative, and Planner requests into one bounded `RetrievalPlan`.
- Replace flat retrieved items with `RetrievedContext { writer, characters }`, enforce audience rules before candidate lookup, retain provenance, and apply deterministic ranking and budgets.
- Route Planner, Character Think, Generator, Repairer, and narrative validation through fixed `PromptProfile` constructors and typed canonical JSON User context.
- Re-enable Retrieval and Character Think from the final request collections and remove temporary unconditional skips.
- Add configuration validation, typed errors, tracing, persistence behavior, tests, and static checks required by these contracts.
- Update the affected sections of `doc/design/2026-08-04-Architecture-gpt.md` so the architecture no longer describes the superseded context model.

### 2.2 Non-Goals

- Do not choose a Summary model, add a Summary LLM call, or define when Summary compaction is scheduled.
- Do not implement BM25 tokenization, a BM25 index, Embedding generation, a vector database, RRF, or learned ranking.
- Do not add `enable_bm25`, `enable_embedding`, `top_k`, provider selection, or retrieval budgets to Story Pack, World Book, save, Planner output, or Player Input.
- Do not add recursive World Book entry activation, scan-depth semantics, Prompt insertion positions, message roles, or SillyTavern compatibility.
- Do not change the fixed Turn Pipeline order, Story-level serialization, validation/repair budget, or atomic commit authority.
- Do not add a second Planner LLM call after Retrieval.
- Do not add multiplayer control assignment or change the current single-player Story Pack contract.
- Do not change the HTTP or WebSocket Turn request/response protocol.
- Do not prescribe a future BM25 tokenizer, Embedding model, vector-store schema, or ranking weights.

### 2.3 Implementation Constraints

- This is a hard refactor under `R-REFACTOR-01/02`. Final code must contain no legacy fallback, dual Snapshot, dual Context model, compatibility alias, or feature flag.
- Delete superseded types, fields, functions, tests, Prompt assembly, Store reads, config fields, and dead comments in the same change.
- `TurnRuntime` remains the only Pipeline orchestrator. No Pipeline may call another Pipeline.
- Every Pipeline continues to implement `TurnExecutionPipeline` and communicates only through `&mut TurnExecutionContext`.
- `turn` may depend on `domain`; `turn` must not import `context`, `planning`, `character`, `story`, `validation`, `runtime`, or persistence adapters.
- Domain code must contain zero turn imports.
- Imported assets, Story state, Player Input, Retrieved Context, Character Thought, and LLM output are untrusted data. Only the internal Prompt module may create a System message.
- All collections and text fields introduced by this spec require explicit count, byte, and/or token limits from trusted `AiseConfig`.
- No write lock may cross `.await`; no Store, channel, trace sink, or LLM side effect may run while a write lock is held.
- All completion and future Embedding calls must use the shared `LlmGateway` limiter, accounting, deadline, cancellation, and tracing transaction.
- Do not add a dependency unless the contract cannot be implemented with the standard library and crates already present in the workspace.
- Code must follow `AGENTS.md`: no code comments, no inline test modules, `mod.rs`/`lib.rs` index-only, compact imports, `forbid(unsafe_code)`, format and Clippy clean.

This spec supersedes the two-Snapshot preservation rule in `2026-08-08-domain-core-dependency-removal-spec-gpt.md` §3.8/§4 R-12. That earlier rule was explicitly temporary pending a separate Story Instance read-model design; the source design for this spec supplies that decision.

It also supersedes `2026-08-07-story-pack-v3-spec-gpt.md` §3.10 where `CurrentPerception` was a `KnowledgeKind`/`KnowledgeSourceId`, and §3.11 where the v3 Snapshot eagerly held all Fact, Rumor, and Memory bodies. All other Story Pack v3 trust, binding, materialization, and commit rules remain authoritative.

### 2.4 Required Implementation Order

1. Add the Domain sequencing, scene, constraint, Topic Dictionary, and knowledge metadata contracts.
2. Add the persistence migration, sole Story Instance Snapshot loader, `KnowledgeSnapshotRef`, and indexed `KnowledgeReadPort`.
3. Replace `domain::turn` and update `TurnExecutionContext`/`TurnBudget` to the final bounded contracts.
4. Implement continuity validation, character views, Topic matching, and retrieval signal extraction in `BaselineContextBuilder`.
5. Integrate `NarrativeDirector`, strict Planner output, typed Prompt requests, and deterministic `RetrievalPlanBuilder`.
6. Implement Entity/Topic Candidate Retrievers, audience filtering, ranking, deduplication, and budget trimming.
7. Integrate audience-specific Character Think, Generator, Repairer, and Validator contexts.
8. Remove the legacy Snapshot, Context, ContextMerger, config fields, Store paths, tests, and temporary stage-disable code.
9. Update architecture documentation, run all static checks, then run the full workspace toolchain.

---

## 3. Contracts

### 3.1 Final File and Module Layout

```text
crates/aise/src/
├── turn/
│   ├── turn_context.rs
│   ├── turn_budget.rs
│   ├── token_estimator.rs
│   └── turn_data/
│       ├── mod.rs
│       ├── baseline.rs
│       ├── character.rs
│       ├── planning.rs
│       └── retrieval.rs
├── context/
│   ├── mod.rs
│   ├── baseline_ctx_builder.rs
│   ├── candidate_retriever.rs
│   ├── entity_candidate_retriever.rs
│   ├── error.rs
│   ├── retrieval_pipeline.rs
│   ├── retrieval_signal_builder.rs
│   ├── tests/
│   │   ├── baseline_ctx_builder_tests.rs
│   │   ├── retrieval_pipeline_tests.rs
│   │   ├── retrieval_signal_builder_tests.rs
│   │   └── topic_matcher_tests.rs
│   ├── topic_candidate_retriever.rs
│   └── topic_matcher.rs
├── planning/
│   ├── mod.rs
│   ├── planner_output.rs
│   ├── retrieval_plan_builder.rs
│   ├── tests/
│   │   ├── retrieval_plan_builder_tests.rs
│   │   └── writer_planner_tests.rs
│   └── writer_planner.rs
├── prompt/
│   ├── model_request.rs
│   ├── profile.rs
│   ├── runtime_context_encoder.rs
│   └── trusted_prompt_source.rs
├── domain/
│   ├── asset/
│   │   ├── ids.rs
│   │   ├── story_pack.rs
│   │   ├── validation.rs
│   │   └── world_book.rs
│   ├── knowledge/
│   │   ├── entry.rs
│   │   ├── fact.rs
│   │   ├── memory.rs
│   │   ├── mod.rs
│   │   ├── query.rs
│   │   └── rumor.rs
│   ├── narrative.rs
│   ├── narrative_graph/
│   │   └── director.rs
│   └── story_instance/
│       ├── constraint.rs
│       ├── mod.rs
│       ├── snapshot.rs
│       └── state.rs
├── persistence/
│   ├── knowledge_read_port.rs
│   ├── mod.rs
│   ├── sqlite_knowledge_reader.rs
│   ├── sqlite_store.rs
│   └── store.rs
└── config.rs

crates/aise/assets/persistence/mig/
└── 0009_context_retrieval.sql
```

Required deletions:

```text
crates/aise/src/turn/turn_data.rs
crates/aise/src/context/context_item.rs
crates/aise/src/domain/story_state.rs
crates/aise/src/prompt/context_merger.rs
crates/aise/src/prompt/tests/context_merger_tests.rs
```

`domain/turn/mod.rs`, `context/mod.rs`, `planning/mod.rs`, `domain/knowledge/mod.rs`, `domain/story_instance/mod.rs`, and `persistence/mod.rs` contain only declarations, re-exports, and item attributes.

### 3.2 Story Sequence and Continuity

`crates/aise/src/domain/narrative.rs` owns story-text order independently from `StoryRevision`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorySequence(u64);

impl StorySequence {
    pub fn try_new(value: u64) -> Result<Self, StoryContinuityError>;
    pub fn get(self) -> u64;
    pub fn next(self) -> Result<Self, StoryContinuityError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorySegment {
    pub sequence: StorySequence,
    pub turn_id: TurnId,
    pub text: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: BoundedText,
    pub summarized_through: Option<StorySequence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryContinuity {
    summary: StorySummary,
    recent_segments: Vec<StorySegment>,
}

impl StoryContinuity {
    pub fn try_new(
        summary: StorySummary,
        recent_segments: Vec<StorySegment>,
        limits: StoryContinuityLimits,
    ) -> Result<Self, StoryContinuityError>;

    pub fn summary(&self) -> &StorySummary;
    pub fn recent_segments(&self) -> &[StorySegment];
    pub fn latest_sequence(&self) -> Option<StorySequence>;
    pub fn next_sequence(&self) -> Result<StorySequence, StoryContinuityError>;
    pub fn estimate_tokens(&self) -> u64;
}

#[derive(Debug, Clone, Copy)]
pub struct StoryContinuityLimits {
    pub max_summary_bytes: usize,
    pub max_recent_segments: usize,
    pub max_recent_segment_bytes: usize,
    pub max_recent_segment_tokens: u64,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StoryContinuityError {
    #[error("story sequence must be positive")]
    ZeroSequence,
    #[error("story sequence overflow")]
    SequenceOverflow,
    #[error("story summary text and summarized_through must either both be present or both be absent")]
    InvalidSummaryBoundary,
    #[error("recent story segments are not strictly ordered")]
    OutOfOrder,
    #[error("story summary and recent story overlap")]
    Overlap,
    #[error("story continuity contains a sequence gap")]
    Gap,
    #[error("story continuity limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
}
```

`StoryTurn` remains the persisted complete Turn record and adds the same sequence:

```rust
pub struct StoryTurn {
    pub id: TurnId,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}
```

The Summary boundary is engine-owned. An LLM may propose Summary text but must never output `summarized_through`; deterministic validation assigns the last pre-Turn `StorySequence` when an existing Summary-change path is accepted.

`StoryContinuity::try_new` enforces these exact rules:

- Summary text is considered empty after trimming. Empty text requires `summarized_through == None`; non-empty text requires `Some(K)`.
- Without a Summary, an empty Recent list represents no committed Story text; otherwise the first Recent sequence is `1`.
- With Summary through `K`, an empty Recent list is valid; otherwise the first Recent sequence is `K + 1`.
- Every later Recent sequence equals its predecessor plus one. A duplicate or descending sequence returns `OutOfOrder`; an ascending jump returns `Gap`; a first sequence at or before `K` returns `Overlap`.
- `latest_sequence` returns the last Recent sequence, otherwise the Summary boundary, otherwise `None`; `next_sequence` returns `latest + 1`, or `1` for an empty Story.
- `max_recent_segment_tokens` limits the sum of deterministic token estimates for all Recent segments. No constructor truncates or drops a segment to satisfy a limit.

### 3.3 Story Instance State, Scene, Settings, and Constraints

Add the following asset keys in `domain/asset/ids.rs`:

```rust
key_type!(ConstraintKey);
key_type!(InstanceSettingKey);
```

`domain/story_instance/state.rs` owns instance settings and the structured scene:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceSettings {
    #[serde(default)]
    pub values: BTreeMap<InstanceSettingKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentScene {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub time: BoundedText,
    pub description: BoundedText,
    pub present_character_ids: Vec<CharacterId>,
}
```

`domain/story_instance/constraint.rs` owns the only active Story constraint model:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryConstraintDefinition {
    pub scope: StoryConstraintScope,
    pub requirement: StoryConstraintRequirement,
    pub lifecycle: StoryConstraintLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintScope {
    Story,
    Scene { scene_key: SceneKey },
    Role { role_key: StoryRoleKey },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintRequirement {
    Require { statement: BoundedText },
    Forbid { statement: BoundedText },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintLifecycle {
    Persistent,
    ThroughSequence { sequence: StorySequence },
    UntilNarrativeNodeResolved { node_key: NarrativeNodeKey },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryConstraintSource {
    Pack { pack_id: PackId, constraint_key: ConstraintKey },
    CommittedTurn { turn_id: TurnId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveStoryConstraint {
    pub id: ConstraintId,
    pub source: StoryConstraintSource,
    pub scope: StoryConstraintScope,
    pub requirement: StoryConstraintRequirement,
    pub lifecycle: StoryConstraintLifecycle,
}
```

Add the optional Pack definitions to `StoryPack`:

```rust
pub struct StoryPack {
    #[serde(default)]
    pub constraints: BTreeMap<ConstraintKey, StoryConstraintDefinition>,
}
```

Pack constraint definitions are materialized once into `ActiveStoryConstraint` values during Story Instance creation. Runtime additions or replacements require the existing Proposal → deterministic Validation → `ValidatedChangeSet` → Commit path. The current active set is always persisted Story Instance state; Baseline Builder never activates or expires constraints on its own.

### 3.4 Topic Dictionary and Knowledge Metadata

Extend `WorldBook` with a centralized Topic Dictionary and rename Entry `tags` fields to `topics`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBook {
    pub spec: WorldSpec,
    pub spec_version: AssetSpecVersion,
    pub world_book_key: WorldBookKey,
    pub meta: WorldBookMeta,
    #[serde(default)]
    pub topics: BTreeMap<TopicKey, TopicDefinition>,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicDefinition {
    pub label: BoundedText,
    #[serde(default)]
    pub aliases: Vec<BoundedText>,
}

pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
}

pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub salience: u8,
}
```

Use one typed Entity reference for indexing and requests:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeEntity {
    World(EntityKey),
    Role(StoryRoleKey),
    Character(CharacterId),
    Location(LocationKey),
    Scene(SceneKey),
    NarrativeNode(NarrativeNodeKey),
    Event(CanonicalEventKey),
}
```

Retain the existing bytewise string `PartialOrd`/`Ord` implementations on ID and asset-key newtypes so typed Entity, audience, source, and map ordering is stable without string copies.

`KnowledgeEntity` replaces `EntityRef`. `Proposition.subject`, `GlobalEventIntentDefinition.participants`, and every other typed Entity reference use `KnowledgeEntity`; final production code contains no `EntityRef` type or conversion layer.

Imported Pack entries must reject `KnowledgeEntity::Character`; Pack definitions use stable Role keys and runtime resolves them through immutable Role bindings. Runtime-generated entries may use `Character`.

All runtime knowledge types retain the same index metadata:

```rust
pub struct WorldFact {
    pub id: FactId,
    pub key: Option<FactKey>,
    pub text: BoundedText,
    pub proposition: Option<Proposition>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
}

pub struct SharedRumor {
    pub id: RumorId,
    pub key: Option<RumorKey>,
    pub content: BoundedText,
    pub claim: Option<Claim>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source_role_key: Option<StoryRoleKey>,
    pub source_character_id: Option<CharacterId>,
    pub truth_value: TruthValue,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
}

pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
    pub created_at_ms: i64,
}

pub enum KnowledgeSource {
    Seed {
        pack_id: PackId,
        pack_digest: Sha256Digest,
    },
    CommittedTurn {
        turn_id: TurnId,
        event_id: Option<EventId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Fact,
    Rumor,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum KnowledgeSourceId {
    Fact(FactId),
    Rumor(RumorId),
    Memory(MemoryId),
}
```

`CurrentPerception` remains a separate transient type keyed by `source_event_id`; it is not a `KnowledgeKind`, `KnowledgeSourceId`, Candidate record, or World Book Entry.

Story Instance creation materializes every Fact seed, Rumor seed, and Role Memory seed exactly once with deterministic IDs derived from `StoryId + stable seed key`; it records `KnowledgeSource::Seed` with the pinned Pack digest. It must not leave seed knowledge only in `facts_json`, `rumors_json`, or `memories_json` arrays that require a full scan.

Topic Dictionary validation must enforce:

- At most `assets.max_topics` definitions.
- At most `assets.max_topic_aliases_per_topic` aliases per Topic.
- Each label/alias is non-empty after trimming and within `assets.max_text_bytes`.
- Normalize each label/alias by Unicode lowercase, trim, and whitespace collapse.
- A normalized label/alias maps to exactly one `TopicKey`; collisions are `AssetValidationCode::DuplicateKey`.
- Every Entry Topic key resolves in the same World Book.

### 3.5 Sole Story Snapshot and Knowledge Scope

`domain/story_instance/snapshot.rs` owns the only `StoryReadSnapshot`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSnapshotRef {
    pub story_id: StoryId,
    pub pack_digest: Sha256Digest,
    pub base_revision: StoryRevision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeConditionStateView {
    pub occurred_event_keys: BTreeSet<CanonicalEventKey>,
    pub player_action_event_keys: BTreeSet<CanonicalEventKey>,
    pub fact_values: BTreeMap<FactKey, ScalarValue>,
}

#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    pack: FrozenStoryPackRef,
    story_profile: StoryProfile,
    instance_settings: InstanceSettings,
    role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    character_cards: BTreeMap<CharacterId, CharacterCard>,
    character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    current_scene: CurrentScene,
    relationships: Vec<RelationshipState>,
    current_perceptions: Vec<CurrentPerception>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    condition_state: NarrativeConditionStateView,
    story_continuity: StoryContinuity,
    active_constraints: Vec<ActiveStoryConstraint>,
    entity_catalog: Vec<KnowledgeEntity>,
    topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    knowledge_snapshot: KnowledgeSnapshotRef,
}

impl StoryReadSnapshot {
    pub fn story_id(&self) -> &StoryId;
    pub fn base_revision(&self) -> StoryRevision;
    pub fn pack(&self) -> &FrozenStoryPackRef;
    pub fn story_profile(&self) -> &StoryProfile;
    pub fn instance_settings(&self) -> &InstanceSettings;
    pub fn role_definitions(&self) -> &BTreeMap<StoryRoleKey, StoryRole>;
    pub fn role_bindings(&self) -> &BTreeMap<StoryRoleKey, RoleBinding>;
    pub fn character_cards(&self) -> &BTreeMap<CharacterId, CharacterCard>;
    pub fn character_states(&self) -> &BTreeMap<CharacterId, CharacterInstanceState>;
    pub fn current_scene(&self) -> &CurrentScene;
    pub fn relationships(&self) -> &[RelationshipState];
    pub fn current_perceptions(&self) -> &[CurrentPerception];
    pub fn narrative_definition(&self) -> &NarrativeGraphDefinition;
    pub fn narrative_state(&self) -> &NarrativeRuntimeState;
    pub fn condition_state(&self) -> &NarrativeConditionStateView;
    pub fn story_continuity(&self) -> &StoryContinuity;
    pub fn active_constraints(&self) -> &[ActiveStoryConstraint];
    pub fn entity_catalog(&self) -> &[KnowledgeEntity];
    pub fn topic_dictionary(&self) -> &BTreeMap<TopicKey, TopicDefinition>;
    pub fn knowledge_snapshot(&self) -> &KnowledgeSnapshotRef;
}
```

The Snapshot contains no Fact, Rumor, or Memory bodies and no `WorldState` lore copy. `entity_catalog` is a bounded, sorted, distinct metadata projection from the Entity index. `NarrativeConditionStateView` contains only bounded structured values required for deterministic graph conditions. `current_perceptions` is bounded transient perception data, not a fourth persistent knowledge class.

`KnowledgeSnapshotRef.story_id` equals Snapshot `story_id`, `pack_digest` equals `pack.digest`, and `base_revision` equals Snapshot `base_revision`. Snapshot construction fails on any mismatch.

`domain/turn/baseline.rs` owns the exact Store read limits:

```rust
#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub max_story_profile_bytes: usize,
    pub max_instance_settings: usize,
    pub max_instance_setting_bytes: usize,
    pub max_roles: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_scene_characters: usize,
    pub max_relationships: usize,
    pub max_current_perceptions: usize,
    pub max_perception_bytes: usize,
    pub max_narrative_nodes: usize,
    pub max_condition_event_keys: usize,
    pub max_condition_fact_values: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_topics: usize,
    pub max_topic_aliases_per_topic: usize,
    pub max_entity_catalog: usize,
    pub continuity: StoryContinuityLimits,
}

impl SnapshotLimits {
    pub fn from_config(
        content: &TurnContentLimitsConfig,
        context: &ContextPreparationConfig,
        assets: &AssetLimitsConfig,
    ) -> Self;
}
```

The Store applies every limit while reading and before allocating the final collection. It never loads an unbounded collection and then checks its length.

### 3.6 Baseline and Character Views

`domain/turn/baseline.rs` defines the final Baseline:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct CharacterView {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub role: StoryRole,
    pub binding: RoleBinding,
    pub card: CharacterCard,
    pub state: CharacterInstanceState,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterIndexEntry {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub name: BoundedText,
    pub narrative_function: BoundedText,
    pub location_key: LocationKey,
    pub player_controlled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeStateView {
    pub pack_digest: Sha256Digest,
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineContext {
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub player_character: CharacterView,
    pub current_scene: CurrentScene,
    pub scene_characters: Vec<CharacterView>,
    pub character_index: Vec<CharacterIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_state_view: NarrativeStateView,
    pub retrieval_signals: RetrievalSignals,
}

impl BaselineContext {
    pub fn estimate_tokens(&self) -> u64;
}
```

`player_character` resolves from the exactly one `RoleBinding` whose `player_id` is present. `scene_characters` follows `CurrentScene.present_character_ids` order after duplicate rejection. `character_index` contains only off-scene characters and is ordered by `CharacterId`.

Baseline must contain none of these legacy values:

```text
story_instructions
StoryConfig
relevant_characters
Vec<String> recent_story
String story_summary
Vec<String> active_constraints
Player Input text
Fact/Rumor/Memory bodies
```

### 3.7 Retrieval Signals and Topic Matching

`domain/turn/retrieval.rs` defines signal values; it stores resolved keys and origin metadata, never source text:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSignalOrigin {
    PlayerInput,
    Scene,
    Narrative,
    RecentStory,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntitySignal {
    pub entity: KnowledgeEntity,
    pub origin: RetrievalSignalOrigin,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopicSignal {
    pub topic: TopicKey,
    pub origin: RetrievalSignalOrigin,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalSignals {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub present_character_ids: Vec<CharacterId>,
    pub active_role_keys: Vec<StoryRoleKey>,
    pub entities: Vec<EntitySignal>,
    pub topics: Vec<TopicSignal>,
}
```

`RetrievalSignalBuilder` has no I/O and no LLM dependency:

```rust
pub struct RetrievalSignalBuilder {
    config: ContextPreparationConfig,
    topic_matcher: TopicMatcher,
}

impl RetrievalSignalBuilder {
    pub fn build(
        &self,
        snapshot: &StoryReadSnapshot,
        player_input: &str,
    ) -> Result<RetrievalSignals, ContextError>;
}
```

Signal priority is fixed:

| Priority | Origin |
|---:|---|
| `0` | Player Input exact Character name/Role label/known Entity and Topic matches |
| `1` | Current Scene, Location, present Character IDs, and active Role keys |
| `2` | Reserved for Narrative signals added by `RetrievalPlanBuilder` after `NarrativeDirector` |
| `3` | The newest two Recent Story segments, newest first |
| `4` | Story Summary disambiguation |

`TopicMatcher` treats each Topic label and alias as a matching term and normalizes it exactly as World Book validation does. It checks terms by descending normalized character length and then `TopicKey`; ASCII alphanumeric terms require non-alphanumeric boundaries, while terms containing any non-ASCII character use literal normalized substring matching. The result is deduplicated by Topic key and sorted by first priority, then Topic key. It performs no fuzzy, recursive, BM25, or vector matching.

Entity text matching uses the same boundary rule against the bounded `entity_catalog`, Character-card names, and Role labels. A Character name or Role label resolves to `KnowledgeEntity::Role(role_key)` for Pack-authored knowledge; explicit runtime Character IDs resolve to `KnowledgeEntity::Character(character_id)`. World/Location/Scene/Narrative/Event entries match only their stable key text in v1. When one normalized name maps to multiple Roles, all matching Role keys are emitted in stable order and count against `context.max_signal_entities`; input collection order never disambiguates them. Structured scene and Narrative references do not require a text match.

### 3.8 Narrative Evaluation Contract

`NarrativeDirector` remains a pure Domain component. Update its input use so it reads `StoryContinuity.latest_sequence`, `NarrativeConditionStateView`, character state, relationships, bindings, and Narrative runtime state; it must not read knowledge bodies or count `recent_turns`.

All `NarrativePlan` members used in typed contexts derive `Serialize` and remain non-persistent proposals:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryGoal {
    pub summary: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativePlan {
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub active_goals: Vec<StoryGoal>,
    pub global_event_intents: Vec<GlobalEventIntent>,
    pub character_impulses: Vec<CharacterImpulse>,
    pub proposed_transitions: Vec<ProposedNarrativeTransition>,
    pub effect_dispositions: Vec<NarrativeEffectDisposition>,
}
```

`WriterPlanner` calls `NarrativeDirector::evaluate` exactly once before its LLM request. The Planner LLM receives `NarrativePlan` as immutable untrusted context data and cannot return, replace, delete, or relax it.

### 3.9 Planner Output, Retrieval Requests, and Writer Plan

`domain/turn/planning.rs` owns the final Turn plan contracts:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RetrievalAudience {
    GlobalWriter,
    Character { character_id: CharacterId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalRequestOrigin {
    Automatic,
    Narrative,
    Planner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalRequest {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalPlan {
    pub requests: Vec<RetrievalRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThinkRequest {
    pub character_id: CharacterId,
    pub reason: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterStoryGoal {
    pub summary: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriterPlan {
    pub story_goal: WriterStoryGoal,
    pub narrative_plan: NarrativePlan,
    pub retrieval_plan: RetrievalPlan,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}
```

`planning/planner_output.rs` parses only this LLM-owned shape:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutput {
    pub story_goal: WriterStoryGoal,
    #[serde(default)]
    pub context_gaps: Vec<PlannerContextGap>,
    #[serde(default)]
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerContextGap {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    #[serde(default)]
    pub entities: Vec<KnowledgeEntity>,
    #[serde(default)]
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
}
```

Unknown fields fail parsing. In particular, Planner output containing any of these fields is invalid:

```text
use_tag_search
use_bm25
use_embedding
retriever
provider
top_k
token_budget
max_items
```

`RetrievalPlanBuilder` is deterministic and performs no I/O:

```rust
pub struct RetrievalPlanBuilder {
    config: RetrievalConfig,
}

impl RetrievalPlanBuilder {
    pub fn build(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        planner_output: PlannerOutput,
        snapshot: &StoryReadSnapshot,
    ) -> Result<WriterPlan, PlanningError>;
}
```

It builds:

```text
Automatic requests from Baseline RetrievalSignals
+ Narrative requests from explicit NarrativePlan references
+ validated Planner context gaps
= final RetrievalPlan
```

Request construction is exact:

- Each Baseline Entity signal creates one `GlobalWriter` Automatic request for `[Fact, Rumor]` with that Entity and the signal's priority.
- Each Baseline Topic signal creates one `GlobalWriter` Automatic request for `[Fact, Rumor]` with that Topic and the signal's priority.
- Narrative active-node, event, participant, location, Role, and Character references are converted to typed `KnowledgeEntity` values. Each distinct reference creates one `GlobalWriter` Narrative request for `[Fact, Rumor]` with priority `2`.
- Each validated Planner gap creates one Planner request with priority `0`. Planner gaps are the only v1 source of Character-audience and Memory requests; a `CharacterThinkRequest` alone does not imply a knowledge request.
- For every Planner `query_text`, the builder applies the §3.7 Entity and Topic matchers and unions all resolved keys into the request before canonicalization. An unresolved query remains bounded provenance for a future lexical/semantic provider and produces no Entity/Topic candidate in v1.

Internally generated reasons are the fixed values `automatic entity signal`, `automatic topic signal`, and `narrative reference`; they are validated against `planner.max_reason_bytes`. Planner query canonicalization performs Unicode lowercase, trims both ends, and replaces every non-empty run of Unicode whitespace with one ASCII space.

It deduplicates requests by the canonical tuple `(audience, sorted knowledge kinds, sorted entities, sorted topics, canonical query text)`. On collision it retains the lowest `signal_priority`; origin precedence is `Automatic`, then `Narrative`, then `Planner`; the retained reason is from the same winning request. Final order is `signal_priority`, origin, audience (`GlobalWriter` then Character ID), knowledge kinds, Entity keys, Topic keys, canonical query text. Exceeding `retrieval.max_requests` after deduplication fails Planning; requests are never silently truncated.

### 3.10 Knowledge Read Port and Candidate Retriever

`persistence/knowledge_read_port.rs` defines the revision-scoped read projection:

```rust
#[derive(Debug, Clone)]
pub struct KnowledgeFilter {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub allowed_writer_memory_owners: Vec<CharacterId>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeRecord {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub source_revision: StoryRevision,
    pub memory_owner: Option<CharacterId>,
}

#[derive(Debug, Clone)]
pub struct EntityKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub entities: &'a [KnowledgeEntity],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct TopicKnowledgeQuery<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub filter: &'a KnowledgeFilter,
    pub topics: &'a [TopicKey],
    pub limit: usize,
}

#[async_trait]
pub trait KnowledgeReadPort: Send + Sync {
    async fn find_by_entities(
        &self,
        query: EntityKnowledgeQuery<'_>,
    ) -> Result<Vec<KnowledgeRecord>, StoreError>;

    async fn find_by_topics(
        &self,
        query: TopicKnowledgeQuery<'_>,
    ) -> Result<Vec<KnowledgeRecord>, StoreError>;
}
```

The provider identity and match provenance contracts live in `domain/turn/retrieval.rs` so turn never imports `context`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateRetrieverKind {
    Entity,
    Topic,
    Bm25,
    Embedding,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case")]
pub enum CandidateMatch {
    Entity(KnowledgeEntity),
    Topic(TopicKey),
}
```

`context/candidate_retriever.rs` imports those turn contracts and defines the provider extension boundary:

```rust
#[derive(Debug, Clone)]
pub struct CandidateRetrievalRequest<'a> {
    pub snapshot: &'a KnowledgeSnapshotRef,
    pub request: &'a RetrievalRequest,
    pub allowed_writer_memory_owners: &'a [CharacterId],
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ContextCandidate {
    pub record: KnowledgeRecord,
    pub audience: RetrievalAudience,
    pub retriever: CandidateRetrieverKind,
    pub provider_rank: u32,
    pub matches: Vec<CandidateMatch>,
    pub signal_priority: u8,
}

#[async_trait]
pub trait CandidateRetriever: Send + Sync {
    fn kind(&self) -> CandidateRetrieverKind;

    async fn retrieve(
        &self,
        request: CandidateRetrievalRequest<'_>,
    ) -> Result<Vec<ContextCandidate>, ContextError>;
}
```

Concrete constructors:

```rust
pub struct EntityCandidateRetriever {
    knowledge: Arc<dyn KnowledgeReadPort>,
}

impl EntityCandidateRetriever {
    pub fn new(knowledge: Arc<dyn KnowledgeReadPort>) -> Self;
}

pub struct TopicCandidateRetriever {
    knowledge: Arc<dyn KnowledgeReadPort>,
}

impl TopicCandidateRetriever {
    pub fn new(knowledge: Arc<dyn KnowledgeReadPort>) -> Self;
}
```

Do not add `Bm25CandidateRetriever` or `EmbeddingCandidateRetriever` in this change. Their enum variants and trait compatibility are the reserved boundary.

`provider_rank` is one-based, distinct within one provider call, and bounded by the call's `limit`. Entity/Topic providers assign it after stable Source-ID ordering. V1 global ranking does not use this field; deduplication retains the minimum rank per provider so a future rank-fusion spec can use the same Candidate contract without comparing raw scores.

### 3.11 Retrieved Context, Provenance, and Ranking

`domain/turn/retrieval.rs` owns the sole final Context item model:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    Topic,
    Entity,
    EntityAndTopic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelevanceRank {
    pub match_level: MatchLevel,
    pub signal_priority: u8,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextProvenance {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub source: KnowledgeSource,
    pub source_revision: StoryRevision,
    pub audience: RetrievalAudience,
    pub memory_owner: Option<CharacterId>,
    pub matched_by: Vec<CandidateRetrieverKind>,
    pub provider_ranks: BTreeMap<CandidateRetrieverKind, u32>,
    pub matches: Vec<CandidateMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextItem {
    pub content: BoundedText,
    pub provenance: ContextProvenance,
    pub relevance: RelevanceRank,
    pub token_cost: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedContext {
    writer: Vec<ContextItem>,
    characters: BTreeMap<CharacterId, Vec<ContextItem>>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetrievedContextLimits {
    pub max_character_audiences: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_item_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RetrievedContextError {
    #[error("retrieved context audience is invalid")]
    InvalidAudience,
    #[error("retrieved context memory owner is invalid")]
    InvalidMemoryOwner,
    #[error("retrieved context count limit exceeded: {limit}")]
    CountLimit { limit: &'static str },
    #[error("retrieved context item byte limit exceeded")]
    ItemByteLimit,
    #[error("retrieved context audience token limit exceeded")]
    AudienceTokenLimit,
    #[error("retrieved context total token limit exceeded")]
    TotalTokenLimit,
    #[error("retrieved context arithmetic overflow")]
    ArithmeticOverflow,
}

impl RetrievedContext {
    pub fn try_new(
        writer: Vec<ContextItem>,
        characters: BTreeMap<CharacterId, Vec<ContextItem>>,
        limits: RetrievedContextLimits,
    ) -> Result<Self, RetrievedContextError>;

    pub fn writer(&self) -> &[ContextItem];
    pub fn for_character(&self, id: &CharacterId) -> &[ContextItem];
    pub fn characters(&self) -> &BTreeMap<CharacterId, Vec<ContextItem>>;
    pub fn total_items(&self) -> usize;
    pub fn total_tokens(&self) -> u64;
}
```

`RetrievedContextError` is owned by `domain::turn`. `ContextRetrievalPipeline` constructs results only through `try_new`; `TurnExecutionContext::set_retrieved_context` repeats the checks against its own Turn limits before phase advancement.

`domain/text/token_estimator.rs` owns the shared estimator:

```rust
pub fn estimate_text_tokens(text: &str) -> u64;
```

It returns `max(1, ceil(text.chars().count() / 4))` with checked/saturating integer arithmetic. Every `ContextItem.token_cost` equals `estimate_text_tokens(content.as_str())`; Story Continuity, partition/total trimming, and LLM accounting use the same function. Provider-reported token counts are never trusted, and the old duplicate estimators are deleted.

The ranking key is exact and uses no floating-point value:

```text
1. MatchLevel descending: EntityAndTopic, Entity, Topic
2. signal_priority ascending
3. salience descending
4. KnowledgeSourceId ascending
```

Candidates deduplicate by `(RetrievalAudience, KnowledgeSourceId)`. A duplicate merges `matched_by` and `matches`, retains the minimum `provider_rank` for each provider, keeps the lowest signal priority, and recomputes `MatchLevel`. Text-equal Fact, Rumor, and Memory records never deduplicate because their stable source IDs and kinds differ.

### 3.12 Audience Authorization Matrix

Authorization happens before `KnowledgeReadPort` performs an Entity/Topic lookup:

| Request audience | Fact | Rumor | Memory | Current Perception |
|---|---|---|---|---|
| `GlobalWriter` | Allowed | Allowed | Allowed only for `character_think_requests` owners and only when the request explicitly identifies that Character | Not directly exposed in v1 |
| `Character { A }` | Forbidden | Allowed | Allowed only when `MemoryEntry.owner == A` | Only A's bounded perception data, supplied separately |

Additional rules:

- A Character request containing `KnowledgeKind::Fact` fails Planner/plan validation with `knowledge_audience_violation`; it is not silently downgraded.
- A Global Writer Memory request must include `KnowledgeEntity::Character(owner)` and `owner` must occur in final `character_think_requests`; otherwise it fails with `knowledge_audience_violation`.
- `KnowledgeReadPort` receives the already-authorized filter and independently enforces the same ownership predicates.
- Fact retrieval for Writer never creates a Character item.
- Character A never receives Character B's Memory.
- V1 does not expose raw Current Perception directly to Planner, Generator, or Repairer. Character Think receives only its own Current Perception, and Generator receives the resulting bounded Character Thought.
- Generator receives only `RetrievedContext.writer()` plus Character Thoughts; it does not receive raw per-Character retrieved items.
- Validator receives provenance for both partitions and may detect Story text that gives a Character unavailable knowledge.

### 3.13 Retrieval Pipeline

```rust
pub struct ContextRetrievalPipeline {
    config: RetrievalConfig,
    retrievers: Vec<Arc<dyn CandidateRetriever>>,
}

impl ContextRetrievalPipeline {
    pub fn new(
        config: RetrievalConfig,
        retrievers: Vec<Arc<dyn CandidateRetriever>>,
    ) -> Result<Self, ContextError>;
}

#[async_trait]
impl TurnExecutionPipeline for ContextRetrievalPipeline {
    fn stage(&self) -> TurnStage;

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
    ) -> Result<(), TurnExecutionError>;
}
```

The production composition root passes exactly one Entity and one Topic retriever, in that order. Constructor validation rejects duplicate kinds, more than `max_candidate_retrievers`, zero retrievers, and a production set lacking either Entity or Topic.

Execution is sequential in final `RetrievalPlan.requests` order and then retriever order. It does not spawn one task per request/provider. It stops candidate collection at both per-provider and Turn-total caps.

After per-audience sorting and trimming, enforce the Turn-total item/token budget using deterministic round-robin across partitions in this order: Writer, then Character IDs ascending, one item per non-empty partition per round. Within one partition, if the next item exceeds that partition or remaining Turn token budget, stop that partition; do not skip it to select a lower-ranked item.

### 3.14 Typed Stage Contexts and Prompt Boundary

Replace `prompt::ContextMerger` with typed request contexts:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct WriterPlannerContext {
    pub baseline: BaselineContext,
    pub narrative_plan: NarrativePlan,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkContext {
    pub character: CharacterView,
    pub current_scene: CurrentScene,
    pub retrieved_context: Vec<ContextItem>,
    pub current_perception: Vec<CurrentPerception>,
    pub impulses: Vec<CharacterImpulse>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorContext {
    pub baseline: BaselineContext,
    pub writer_plan: WriterPlan,
    pub writer_context: Vec<ContextItem>,
    pub character_thoughts: Vec<CharacterThought>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairerContext {
    pub generation: StoryGeneratorContext,
    pub previous_proposal: StoryProposal,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeValidatorContext {
    pub baseline: BaselineContext,
    pub snapshot_revision: StoryRevision,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub proposal: StoryProposal,
    pub writer_context_provenance: Vec<ContextProvenance>,
    pub character_context_provenance: BTreeMap<CharacterId, Vec<ContextProvenance>>,
}
```

`NarrativeValidatorContext.baseline` is the bounded serializable projection built from the same Snapshot revision; the LLM-facing Validator does not serialize the private Snapshot object itself. Deterministic Validators continue to read the same Snapshot directly from `TurnExecutionContext`.

`domain/turn/character.rs` owns the bounded transient cognition contract:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterThoughtOutput {
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
}
```

`CharacterThoughtOutput` is private to the Character Pipeline. The Pipeline assigns the requested ID after strict parsing and applies `content.max_character_thought_bytes` to the canonical serialized output before storing `CharacterThought`.

`ModelRequest<C>` constructors remain the only way a Pipeline selects a profile:

```rust
impl ModelRequest<WriterPlannerContext> {
    pub(crate) fn writer_planner(context: WriterPlannerContext, max_output_tokens: u32) -> Self;
}

impl ModelRequest<CharacterThinkContext> {
    pub(crate) fn character_think(context: CharacterThinkContext, max_output_tokens: u32) -> Self;
}

impl ModelRequest<StoryGeneratorContext> {
    pub(crate) fn story_generator(context: StoryGeneratorContext, max_output_tokens: u32) -> Self;
}

impl ModelRequest<StoryRepairerContext> {
    pub(crate) fn story_repairer(context: StoryRepairerContext, max_output_tokens: u32) -> Self;
}

impl ModelRequest<NarrativeValidatorContext> {
    pub(crate) fn narrative_validator(context: NarrativeValidatorContext, max_output_tokens: u32) -> Self;
}
```

The internal Prompt module provides the only trusted instruction source:

```rust
pub trait TrustedPromptSource: Send + Sync {
    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError>;
}

pub struct CatalogPromptSource {
    catalog: Arc<PromptCatalog>,
    profiles: PromptProfileRegistry,
}

impl TrustedPromptSource for CatalogPromptSource {
    fn compose(&self, input: &PromptCompositionInput) -> Result<PromptComposition, PromptError>;
}
```

`compose` accepts a fixed `PromptProfile`, bounded RC runtime variables, and trusted FTI variables. The code-owned profile registry selects distinct CSI, RC, and FTI slots; imported content cannot select assets or alter trusted layers. Remove `LlmGateway::with_system_prompts` and ad hoc per-Pipeline fallback prompt construction. The composition root builds `CatalogPromptSource` from trusted `PromptModuleConfig`.

`LlmGateway::complete_typed` owns Prompt resolution, canonical Context encoding, message construction, token estimation/reservation, shared limiting, accounting, and tracing:

```rust
pub fn new(
    provider: Arc<dyn LlmProvider>,
    prompt_source: Arc<dyn TrustedPromptSource>,
    config: LlmConfig,
) -> Result<Self, TurnExecutionError>;

pub async fn complete_typed<C: Serialize>(
    &self,
    mut scope: TurnLlmCallScope<'_>,
    request: ModelRequest<C>,
) -> Result<LlmCompletion, LlmError>;
```

It resolves one trusted System Prompt from `PromptProfile`, encodes `request.context()` with `RuntimeContextEncoder`, creates exactly one System and one User message, reserves the Turn LLM budget from the final encoded messages, and executes the existing Gateway transaction. Business Pipelines do not construct `ChatMessage`, select roles, call `CompletionSpec`, or reserve estimated input tokens themselves.

### 3.15 Pipeline Integration Contracts

`WriterPlanner` final constructor and behavior:

```rust
pub struct WriterPlanner {
    gateway: Arc<LlmGateway>,
    narrative_director: NarrativeDirector,
    plan_builder: RetrievalPlanBuilder,
    config: PlannerConfig,
}

pub fn new(
    gateway: Arc<LlmGateway>,
    narrative_director: NarrativeDirector,
    plan_builder: RetrievalPlanBuilder,
    config: PlannerConfig,
) -> Self;
```

It reads Snapshot, Baseline, and raw Player Input; evaluates Narrative once; calls `ModelRequest::writer_planner`; strictly parses and bounds `PlannerOutput`; builds final `WriterPlan`; and calls `ctx.set_writer_plan` exactly once.

`CharacterThinkPipeline` iterates `WriterPlan.character_think_requests` in `CharacterId` order after deduplication. For each request it:

1. Resolves the full `CharacterView` from the same Snapshot.
2. Rejects a player-controlled binding.
3. Selects only `RetrievedContext.for_character(character_id)`.
4. Selects only that Character's Current Perception and Narrative impulses.
5. Calls `ModelRequest::character_think` through `complete_typed`.
6. Appends one bounded `CharacterThought`.

Calls are sequential and all use the shared Gateway limiter. Unknown Character IDs and player-controlled requests are Planner-output errors; they are not warning-and-skip cases.

`StoryGenerator` calls `ModelRequest::story_generator` with Writer Context only. `StoryRepairer` calls `ModelRequest::story_repairer` with the identical generation context, rejected Proposal, and bounded issues. Active Story Constraints occur in Baseline for Planner, Generator, and Repairer, and explicitly in Validator context.

### 3.16 Turn Execution Context Contract

Replace these stored fields in `TurnExecutionContext`; all identity, phase, request, control, budget, trace, proposal, validation, commit, terminal, and LLM-ledger fields retain their existing ownership:

```rust
snapshot: Option<domain::story_instance::snapshot::StoryReadSnapshot>,
baseline: Option<BaselineContext>,
plan: Option<WriterPlan>,
retrieved: RetrievedContext,
thoughts: Vec<CharacterThought>,
```

Required accessors/mutators:

```rust
pub fn snapshot(&self) -> Option<&StoryReadSnapshot>;
pub fn baseline(&self) -> Option<&BaselineContext>;
pub fn plan(&self) -> Option<&WriterPlan>;
pub fn retrieved(&self) -> &RetrievedContext;

pub fn set_prepared_context(
    &mut self,
    snapshot: StoryReadSnapshot,
    baseline: BaselineContext,
) -> Result<(), TurnExecutionError>;

pub fn set_writer_plan(&mut self, plan: WriterPlan) -> Result<(), TurnExecutionError>;
pub fn set_retrieved_context(&mut self, context: RetrievedContext) -> Result<(), TurnExecutionError>;
pub fn requires_retrieval(&self) -> Result<bool, TurnExecutionError>;
pub fn requires_character_thinking(&self) -> Result<bool, TurnExecutionError>;
```

`requires_retrieval` returns `!plan.retrieval_plan.requests.is_empty()`. `requires_character_thinking` returns `!plan.character_think_requests.is_empty()`. Both require `TurnPhase::Planned` and an existing plan. Delete the temporary `Ok(false)` implementations and their comments.

`set_retrieved_context` validates every partition, per-audience count/tokens, total count/tokens, item bytes, provenance audience, Memory owner, and configured Character audience count before assignment. `skip_retrieval` assigns `RetrievedContext::default()`.

### 3.17 Configuration and Budget Contract

Add typed trusted configuration:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPreparationConfig {
    pub max_scene_characters: usize,
    pub max_character_index: usize,
    pub max_relationships: usize,
    pub max_current_perceptions: usize,
    pub max_condition_event_keys: usize,
    pub max_condition_fact_values: usize,
    pub max_entity_catalog: usize,
    pub max_signal_entities: usize,
    pub max_signal_topics: usize,
    pub recent_segments_for_signals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub max_context_gaps: usize,
    pub max_character_think_requests: usize,
    pub max_goal_bytes: usize,
    pub max_query_bytes: usize,
    pub max_reason_bytes: usize,
    pub max_entities_per_request: usize,
    pub max_topics_per_request: usize,
    pub max_kinds_per_request: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub max_requests: usize,
    pub max_candidate_retrievers: usize,
    pub max_candidates_per_retriever: usize,
    pub max_candidates_total: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_item_bytes: usize,
}

pub context: ContextPreparationConfig,
pub planner: PlannerConfig,
pub retrieval: RetrievalConfig,
```

The three fields above are added to the existing trusted `AiseConfig`. `TurnBudgetLimits` receives the retrieval totals from `RetrievalConfig`; Pipeline code does not retain duplicate total limits. Remove `TurnConfig.max_retrieved_items` and `TurnConfig.max_retrieval_candidates`, then remove these obsolete `TurnContentLimitsConfig` fields:

```text
max_story_instructions_bytes
max_story_config_bytes
max_world_facts
max_world_fact_bytes
max_memories
max_memory_bytes
max_retrieval_candidates
max_retrieved_items
max_retrieved_item_bytes
max_retrieved_tokens
max_character_thoughts
max_trace_spans
```

Keep byte limits that still own a unique meaning and rename/add these:

```rust
pub struct TurnContentLimitsConfig {
    pub max_story_profile_bytes: usize,
    pub max_instance_settings: usize,
    pub max_instance_setting_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_summary_bytes: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_perception_bytes: usize,
    pub max_recent_segments: usize,
    pub max_recent_segment_bytes: usize,
    pub max_recent_segment_tokens: u64,
    pub max_plan_bytes: usize,
    pub max_character_thought_bytes: usize,
    pub max_proposal_bytes: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_field_bytes: usize,
}
```

`content.max_recent_turns` is renamed to `max_recent_segments`, and `content.max_recent_turn_bytes` is renamed to `max_recent_segment_bytes`; neither old name remains as a serde alias. New `AiseConfig.context`, `.planner`, and `.retrieval` fields use `#[serde(default)]` with validated positive defaults.

Extend trusted asset limits:

```rust
pub max_topics: usize,
pub max_topic_aliases_per_topic: usize,
pub max_entities_per_entry: usize,
pub max_topics_per_entry: usize,
```

`TurnBudget` uses one construction path:

```rust
pub fn from_config(
    turn: &TurnConfig,
    content: &TurnContentLimitsConfig,
    retrieval: &RetrievalConfig,
) -> Result<Self, TurnExecutionError>;
```

All numeric fields above must be positive. Also enforce:

- `context.recent_segments_for_signals <= content.max_recent_segments` and `<= 2` for this implementation.
- `context.max_scene_characters <= content.max_characters` and `context.max_character_index <= content.max_characters`.
- `retrieval.max_candidate_retrievers == 2` in the production v1 configuration.
- `retrieval.max_candidates_total >= retrieval.max_candidates_per_retriever`.
- `retrieval.max_candidates_total >= retrieval.max_items_per_audience`.
- `retrieval.max_total_items >= retrieval.max_items_per_audience`.
- `retrieval.max_total_tokens >= retrieval.max_tokens_per_audience`.
- `planner.max_character_think_requests <= turn.max_character_thoughts`.
- Snapshot and asset limits are not derived from Pack-provided values.

### 3.18 Persistence Contract

`Store::load_story_snapshot` returns the sole v3 Snapshot and never loads knowledge bodies:

```rust
#[async_trait]
pub trait Store: Send + Sync {
    async fn create_story_instance(
        &self,
        spec: &MaterializedStoryInstanceSpec,
    ) -> Result<StoryInfo, StoreError>;

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError>;
}
```

Existing `get_story`, idempotency lookup, instance metadata, and commit methods remain on the trait with their current authority; the block above shows the changed read/create signatures only.

Delete legacy `Store::create_story`, `StoryCreateSpec`, `AuthoritativeStoryState`, `PlayerStoryState`, legacy `WorldState` Snapshot reads, and the legacy Story-create API path that exists only to populate `story_instructions`/`StoryConfig`.

Migration `0009_context_retrieval.sql` must provide these semantic structures:

```text
story_turns.sequence
    INTEGER NOT NULL
    UNIQUE(world_id, sequence)

knowledge_entries
    story_id
    source_id
    knowledge_kind
    memory_owner_character_id NULLABLE
    content
    salience
    source_json
    source_revision
    PRIMARY KEY(story_id, knowledge_kind, source_id)

knowledge_entry_entities
    story_id
    knowledge_kind
    source_id
    entity_kind
    entity_key
    PRIMARY KEY(story_id, knowledge_kind, source_id, entity_kind, entity_key)
    INDEX(story_id, entity_kind, entity_key, knowledge_kind, source_id)

knowledge_entry_topics
    story_id
    knowledge_kind
    source_id
    topic_key
    PRIMARY KEY(story_id, knowledge_kind, source_id, topic_key)
    INDEX(story_id, topic_key, knowledge_kind, source_id)
```

`knowledge_kind` accepts only `fact`, `rumor`, or `memory`. `memory_owner_character_id` is non-null exactly for `memory`; mapping rows reference the composite Entry key with `ON DELETE CASCADE`; salience is constrained to `0..=255`; and all ID/key/content/source fields are non-null. Application reads still enforce `retrieval.max_item_bytes` and collection limits before constructing Domain values.

`story_turns.world_id` remains the historical SQL column name in this migration and stores the semantic `StoryId`; no new `story_id` alias column or dual-write path is added to `story_turns`.

Existing `story_turns` rows are backfilled per Story using `(created_at, rowid)` order only during migration. Runtime ordering uses `sequence` exclusively afterward. Summary JSON is upgraded to include `summarized_through`; an empty legacy Summary maps to `None`. Non-empty legacy Summary without a provable coverage boundary must fail migration or require explicit fixture replacement; runtime must not guess a boundary.

`SqliteStore` implements `KnowledgeReadPort` using the Entity/Topic mapping indexes. Each method starts a short read transaction, verifies current Story revision equals `KnowledgeSnapshotRef.base_revision` and pinned Pack digest equals `KnowledgeSnapshotRef.pack_digest`, performs the authorized indexed lookup with SQL `LIMIT`, materializes bounded `KnowledgeRecord` values, and closes the transaction before returning.

No retrieval query may deserialize all `facts_json`, `rumors_json`, `memories_json`, World Book JSON, or all `knowledge_entries` and then filter in Rust. Historical JSON columns in earlier migration files may remain for migration history, but production code has zero reads/writes to them after this change.

`TurnCommitter` assigns `StoryTurn.sequence = snapshot.story_continuity().next_sequence()`; model output cannot provide it. Commit keeps the existing `base_revision` optimistic check and the database unique sequence constraint. A conflict rolls back Story text, knowledge, Summary, constraints, outbox, idempotency result, and LLM ledger together.

### 3.19 Error Contract

```rust
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("story snapshot is inconsistent: {code}")]
    SnapshotInconsistent { code: &'static str },
    #[error("story continuity is invalid: {code}")]
    ContinuityInvalid { code: &'static str },
    #[error("retrieval signal limit exceeded: {limit}")]
    SignalLimitExceeded { limit: &'static str },
    #[error("retrieval plan is invalid: {code}")]
    InvalidPlan { code: &'static str },
    #[error("knowledge audience violation")]
    KnowledgeAudienceViolation,
    #[error("candidate retriever configuration is invalid: {code}")]
    InvalidRetrieverSet { code: &'static str },
    #[error("retrieval candidate limit exceeded")]
    CandidateLimitExceeded,
    #[error("retrieved context budget exceeded: {limit}")]
    RetrievedBudgetExceeded { limit: &'static str },
    #[error("knowledge read failed")]
    Store(StoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum PlanningError {
    #[error("narrative evaluation failed")]
    Narrative(NarrativeError),
    #[error("writer planner output is invalid: {code}")]
    InvalidOutput { code: &'static str },
    #[error("writer planner referenced an unknown character")]
    UnknownCharacter,
    #[error("writer planner requested a player-controlled character")]
    PlayerCharacterRequested,
    #[error("writer planner referenced an unknown entity or topic")]
    UnknownRetrievalKey,
    #[error("writer planner violated knowledge audience rules")]
    KnowledgeAudienceViolation,
    #[error("writer plan limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
}
```

Pipeline boundaries map these typed errors to `TurnExecutionError` with stable codes and the owning stage:

| Error | Turn code | Stage |
|---|---|---|
| Snapshot/continuity failure | `context_snapshot_invalid` | `BaselineBuilder` |
| Baseline/signal limit | `context_baseline_limit` | `BaselineBuilder` |
| Narrative failure | `narrative_evaluation_failed` | `WriterPlanner` |
| Planner JSON/schema/key/audience failure | `writer_plan_invalid` | `WriterPlanner` |
| Knowledge scope revision/digest mismatch | `retrieval_snapshot_conflict` | `ContextRetrieval` |
| Candidate/provider limit | `retrieval_candidate_limit` | `ContextRetrieval` |
| Retrieved Context budget | `retrieval_context_limit` | `ContextRetrieval` |
| Store availability/serialization | existing `store_error` mapping | owning stage |

No error message includes Player Input, Prompt, Memory content, Story text, raw LLM output beyond the existing bounded development-only trace policy, or full imported Entry content.

### 3.20 Affected File Manifest

The minimum production and contract change set is:

```text
doc/design/2026-08-04-Architecture-gpt.md
crates/aise/assets/persistence/mig/0009_context_retrieval.sql
crates/aise/src/config.rs
crates/aise/src/turn/mod.rs
crates/aise/src/turn/turn_budget.rs
crates/aise/src/turn/turn_context.rs
crates/aise/src/turn/token_estimator.rs                  ADD
crates/aise/src/turn/turn_data.rs                         DELETE
crates/aise/src/turn/turn_data/*                         ADD
crates/aise/src/context/*
crates/aise/src/context/context_item.rs                   DELETE
crates/aise/src/domain/asset/ids.rs
crates/aise/src/domain/asset/story_pack.rs
crates/aise/src/domain/asset/validation.rs
crates/aise/src/domain/asset/world_book.rs
crates/aise/src/domain/knowledge/*
crates/aise/src/domain/mod.rs
crates/aise/src/domain/narrative.rs
crates/aise/src/domain/narrative_graph/director.rs
crates/aise/src/domain/story_instance/*
crates/aise/src/domain/story_state.rs                    DELETE
crates/aise/src/engine.rs
crates/aise/src/llm/gateway.rs
crates/aise/src/persistence/*
crates/aise/src/planning/*
crates/aise/src/prompt/context_merger.rs                 DELETE
crates/aise/src/prompt/mod.rs
crates/aise/src/prompt/model_request.rs
crates/aise/src/prompt/trusted_prompt_source.rs
crates/aise/src/prompt/tests/context_merger_tests.rs     DELETE
crates/aise/src/runtime/turn_runtime.rs
crates/aise/src/story/instance_factory.rs
crates/aise/src/story/story_generator.rs
crates/aise/src/story/story_repairer.rs
crates/aise/src/character/character_think_pipeline.rs
crates/aise/src/validation/validation_pipeline.rs
crates/aise/src/validation/validators/knowledge_boundary.rs
crates/aise/src/validation/validators/consistency.rs
crates/aise/tests/*context*.rs
crates/aise/tests/engine_flow_tests.rs
crates/aise/tests/persistence_tests.rs
crates/aise/tests/runtime_tests.rs
crates/aise/tests/story_pack_runtime_tests.rs
crates/aise/tests/trust_boundary_tests.rs
crates/aise-server/src/api/story.rs                    legacy create path DELETE
crates/aise-server/tests/story_api_tests.rs
crates/aise-server/tests/sse_tests.rs
```

Compiler errors and static searches may reveal additional call sites; this manifest is not permission to leave old paths elsewhere.

---

### 3.21 Required Test Contract

Add these integration test files:

```text
crates/aise/tests/story_continuity_tests.rs
crates/aise/tests/context_preparation_retrieval_tests.rs
crates/aise/tests/knowledge_read_port_tests.rs
crates/aise/tests/prompt_context_contract_tests.rs
```

They contain at least these named cases:

| Test | Required assertion |
|---|---|
| `story_sequence_rejects_zero_and_overflow` | `try_new(0)` and `try_new(u64::MAX).and_then(StorySequence::next)` return the exact typed variants |
| `continuity_without_summary_starts_at_one` | An unsummarized story accepts `1..N` and rejects any other first sequence |
| `continuity_summary_and_recent_are_adjacent` | Summary through `K` accepts Recent `K+1..N` |
| `continuity_rejects_overlap_gap_duplicate_and_disorder` | Each invalid shape returns its matching typed error |
| `continuity_budget_never_silently_drops_segments` | Over-budget unsummarized suffix fails instead of truncating |
| `baseline_uses_one_snapshot_and_no_knowledge_bodies` | Builder calls Snapshot load once and no knowledge read method |
| `baseline_resolves_player_scene_and_off_scene_index_by_stable_id` | Player, scene list, and sorted index match bindings/IDs rather than collection order |
| `baseline_does_not_copy_player_input` | Serialized Baseline contains no Player Input field or full input text |
| `topic_dictionary_rejects_normalized_alias_collisions` | Case/whitespace-equivalent aliases under different Topics fail import |
| `topic_matcher_handles_ascii_boundaries_and_chinese_aliases` | ASCII partial words do not match; exact Chinese aliases do match |
| `retrieval_signals_follow_fixed_priority_and_bounds` | Priority/order/deduplication equal §3.7 and overflow fails |
| `narrative_director_uses_continuity_and_condition_view` | Turn/condition evaluation needs no knowledge-body collection |
| `planner_output_rejects_provider_and_budget_fields` | Every forbidden field from §3.9 fails strict deserialization |
| `automatic_requests_run_when_planner_gaps_are_empty` | Non-empty automatic signals produce a non-empty final plan |
| `retrieval_plan_merge_is_deterministic` | Input permutations produce byte-identical canonical JSON plans |
| `planner_query_resolves_known_keys_before_retrieval` | Canonical query text adds exact Entity/Topic keys; unresolved text performs no v1 fallback |
| `planner_cannot_replace_narrative_plan_or_constraints` | Unknown output fields for either value fail parsing |
| `character_fact_request_is_rejected_before_store_lookup` | Knowledge port call count remains zero |
| `writer_memory_requires_planned_owner` | Missing/unplanned owner returns audience violation before lookup |
| `character_memory_is_owner_isolated` | Character A receives no Character B Memory from either provider |
| `fact_retrieval_never_creates_character_context` | Fact remains Writer-only even when the same Entity matches |
| `entity_topic_duplicate_merges_match_reasons` | One source appears once with `EntityAndTopic` rank |
| `candidate_provider_rank_is_bounded_and_not_used_by_v1_ranking` | Provider ranks are one-based/bounded and cannot change the §3.11 order |
| `context_and_llm_accounting_share_one_token_estimator` | Context items, continuity, and LLM accounting produce the exact §3.11 estimate |
| `conflicting_fact_rumor_and_memory_remain_distinct` | Equal text with different source IDs/kinds yields three items when authorized |
| `ranking_uses_exact_stable_order` | Match level, signal priority, salience, and source ID order equal §3.11 |
| `retrieval_budget_round_robin_is_deterministic` | Writer/Character trimming order and token stops equal §3.13 |
| `zero_result_request_never_falls_back_to_full_scan` | Only indexed query methods are called; result stays empty |
| `knowledge_read_rejects_revision_or_digest_mismatch` | Both mismatches return the snapshot conflict path |
| `sqlite_entity_and_topic_queries_use_indexes` | `EXPLAIN QUERY PLAN` identifies the mapping indexes and no full `knowledge_entries` scan |
| `typed_context_emits_one_trusted_system_and_one_untrusted_user_message` | Exact roles/count/profile are asserted for every stage |
| `asset_and_player_content_never_enters_system_prompt` | Adversarial strings occur only in encoded User JSON |
| `retrieval_and_character_think_are_enabled_from_plan_collections` | Empty collections skip; non-empty collections execute exactly once |
| `generator_receives_writer_items_not_raw_character_items` | Serialized Generator Context excludes Character partitions |
| `turn_commit_assigns_next_story_sequence` | Sequence is `N+1` and ignores model/provider payloads |
| `context_retrieval_end_to_end_is_revision_consistent` | Snapshot, retrieved source revisions, proposal validation, and commit share one base revision |

Unit tests for each new source file live under that module's `tests/<source>_tests.rs` and start with `use super::*;`. Existing tests must be migrated; do not retain legacy fixtures solely to keep the old path compiling.

---

## 4. Behavior Rules

1. **CPR-1 — Sole Snapshot**: Production Turn code defines and uses exactly one `StoryReadSnapshot`, at `domain/story_instance/snapshot.rs`.
2. **CPR-2 — One Snapshot Load**: `BaselineContextBuilder` performs exactly one `Store::load_story_snapshot` call per non-replayed Turn and no knowledge-body read.
3. **CPR-3 — Snapshot Scope**: Snapshot Story ID, Pack digest, and base revision exactly equal its `KnowledgeSnapshotRef` values.
4. **CPR-4 — No Long Read Transaction**: Snapshot and Candidate reads use short transactions that end before any LLM call.
5. **CPR-5 — Story Order**: Story text order uses `StorySequence`; timestamp and `StoryRevision` never determine Summary coverage or Recent Story order.
6. **CPR-6 — Sequence Assignment**: Only `TurnCommitter` assigns the next Story sequence, and the database enforces uniqueness per Story.
7. **CPR-7 — Continuity**: Summary and Recent Story form one contiguous prefix ending at the latest committed segment with no overlap, gap, duplicate, or reordering.
8. **CPR-8 — No Silent History Loss**: If the unsummarized suffix exceeds configured limits, preparation fails; Builder never drops arbitrary segments.
9. **CPR-9 — Player Input Ownership**: `TurnRequest` is the authoritative Turn owner of Player Input; only an ephemeral typed stage request may carry a bounded copy, and Baseline, Snapshot, and Writer Plan contain none.
10. **CPR-10 — Baseline Contents**: Baseline contains only the fields in §3.6 and no Prompt, raw message, knowledge body, or full off-scene Character view.
11. **CPR-11 — Stable Character Resolution**: Player, scene, and off-scene Character views resolve through stable bindings and IDs, never SQL/HashMap iteration order.
12. **CPR-12 — Structured Constraints**: All active constraints use stable ID, source, scope, typed requirement, and lifecycle; no `Vec<String>` constraint path remains.
13. **CPR-13 — Constraint Authority**: LLM output cannot add, remove, replace, activate, expire, or relax a constraint except through a validated proposed change and atomic commit.
14. **CPR-14 — Prompt Separation**: Prompt module supplies trusted System instructions; every stage Context is canonical JSON User data and cannot select a role or Prompt asset.
15. **CPR-15 — Fixed Profiles**: Planner, Character Think, Generator, Repairer, and Narrative Validator use only their corresponding `PromptProfile` constructors.
16. **CPR-16 — Planner Order**: Narrative evaluation completes before the single Planner LLM call; Retrieval occurs after that call.
17. **CPR-17 — Planner Authority Limit**: Planner output contains Story Goal, Context Gaps, and Character Think requests only; it cannot return Narrative Plan, constraints, algorithms, budgets, or providers.
18. **CPR-18 — Automatic Retrieval**: Retrieval executes when the final plan has any request, including when Planner gaps are empty but Automatic or Narrative requests exist.
19. **CPR-19 — Deterministic Signals**: Signal extraction scans only bounded Player Input, structured scene data, two newest Recent segments, and Summary; it stores only resolved keys/origins.
20. **CPR-20 — Topic Dictionary**: Natural-language Topic resolution uses the validated centralized dictionary and the exact deterministic normalization/matching algorithm in §3.7.
21. **CPR-21 — Entry Metadata**: Every Fact, Rumor, and Memory retains stable source ID, Entity refs, Topic keys, salience, kind, source, and source revision.
22. **CPR-22 — Derived Indexes**: Entity and Topic indexes are rebuildable projections; authoritative knowledge remains the typed Entry records.
23. **CPR-23 — Provider Isolation**: A Candidate Retriever returns candidates only; it does not apply global ranking, final deduplication, cross-provider score addition, or budget trimming.
24. **CPR-24 — Implemented Providers**: Production registers Entity and Topic providers only. BM25 and Embedding have no implementation, startup switch, or empty-result stub.
25. **CPR-25 — Audience First**: Audience and knowledge-kind authorization complete before any Candidate query.
26. **CPR-26 — Character Knowledge**: Character Think receives only relevant Rumor, its own Memory, and its own Current Perception; a canonical Fact is never exposed merely because Writer saw it.
27. **CPR-27 — Writer Memory Bound**: Writer Memory access is restricted to explicitly identified Characters already selected for Character Think.
28. **CPR-28 — Partitioned Result**: Retrieved Context stores Writer items and each Character's items in separate partitions; there is no flat intermediate result exposed to stages.
29. **CPR-29 — Conflict Preservation**: Fact, Rumor, and Memory records do not merge across semantic kinds even when content is identical or contradictory.
30. **CPR-30 — Stable Deduplication**: Candidate deduplication uses `(audience, source_id)` and merges only match provenance.
31. **CPR-31 — Stable Ranking**: Initial ranking uses only Match level, signal priority, salience, and stable Source ID; it uses no `f32`, random value, timestamp, or provider-return order.
32. **CPR-32 — Bounded Retrieval**: Requests, providers, candidates per provider, total candidates, items per audience, tokens per audience, total items, total tokens, and item bytes all stop at trusted limits.
33. **CPR-33 — No Full-Scan Fallback**: A zero-result or unresolved query returns zero items and emits metadata; it never scans or injects the full World Book.
34. **CPR-34 — Deterministic Execution**: V1 Candidate calls are sequential in canonical request/provider order; no per-request task fan-out is added.
35. **CPR-35 — Character Requests**: Character Think requests are unique, bounded, known, and AI-controlled; unknown/player-controlled IDs fail Planning instead of warning and skipping.
36. **CPR-36 — Transient Thought**: Character Thought remains Turn-scoped, is not a Fact, and cannot be committed without a separate validated proposed change.
37. **CPR-37 — Generator Visibility**: Generator sees Writer Context and Character Thoughts, never raw Character-specific Memory/Rumor partitions.
38. **CPR-38 — Provenance Preservation**: Generator and Validator retain source ID, kind, source revision, audience, Memory owner, match providers, and match reasons for every retrieved item.
39. **CPR-39 — Optional Stages**: `requires_retrieval` and `requires_character_thinking` derive exclusively from final request collections; no separate booleans exist.
40. **CPR-40 — Pipeline Isolation**: Pipelines neither invoke one another nor retain another Pipeline instance.
41. **CPR-41 — LLM Gateway**: All stage LLM calls use `complete_typed`; Pipeline code does not construct provider messages or reserve LLM input tokens.
42. **CPR-42 — Revision Read**: Every Candidate read verifies Story revision and Pack digest before returning records.
43. **CPR-43 — Atomic Sequence Commit**: Sequence, Story text, Summary boundary, knowledge changes, constraints, Narrative changes, outbox, idempotency result, and LLM ledger commit or roll back together.
44. **CPR-44 — Hard Replacement**: Legacy Snapshot, `StoryConfig`, `story_instructions`, `ContextSource`, `ContextRequest`, flat `ContextItem`, `ContextMerger`, unconditional stage skips, and production JSON knowledge scans have zero references after the change.
45. **CPR-45 — No Asset Policy**: Story Pack/World Book/save cannot contain Prompt, model, retrieval-provider, algorithm, budget, concurrency, timeout, validation-policy, tool, or skill controls.

### 4.1 Error Handling

- `StorySequence::try_new(0)` returns `StoryContinuityError::ZeroSequence` with `story sequence must be positive`.
- `StorySequence::try_new(u64::MAX).and_then(StorySequence::next)` returns `StoryContinuityError::SequenceOverflow` with `story sequence overflow`.
- Empty Summary text with a boundary, or non-empty Summary text without a boundary, returns `InvalidSummaryBoundary`.
- Gap and overlap return their exact variants; duplicate and descending Recent sequences return `OutOfOrder`. All fail before any LLM or Candidate call.
- Missing player binding, duplicate player binding, missing Character card/state, invalid scene Character ID, or Snapshot-scope mismatch returns `context_snapshot_invalid` at Baseline stage.
- Unknown Planner Character/Entity/Topic keys and forbidden Planner fields return `writer_plan_invalid`; raw output is not included in the production error.
- Audience violations return `writer_plan_invalid` when introduced by Planner output and `retrieval_snapshot_conflict`/`retrieval_context_limit` only for later invariant corruption.
- Revision or Pack digest mismatch during Candidate reads returns `StoreError::RevisionConflict`, mapped to `retrieval_snapshot_conflict`; no stale items are returned.
- Invalid persisted knowledge metadata returns `StoreError::Serialization`; it is never silently skipped or mapped to availability.
- No external, persisted, asset, or LLM input path uses `unwrap`, `expect`, or `panic`.
- Non-fatal zero-result requests complete successfully and record bounded metadata; Store/provider/config/invariant failures abort the Turn before generation.

### 4.2 Concurrency

- `BaselineContextBuilder` awaits one Snapshot load and owns no lock or transaction.
- Topic matching, signal extraction, Narrative evaluation, request merging, deduplication, ranking, and trimming are synchronous bounded work.
- Retrieval awaits Candidate providers sequentially and never holds a lock or Store transaction between calls.
- `SqliteStore` ends each Snapshot/Candidate transaction before returning to a Pipeline.
- Character Think calls are sequential in stable Character order for this implementation; every call still passes through the shared Gateway limiter.
- No new background task, channel, cache, queue, detached future, or unbounded `join_all` is added.
- A future Embedding Candidate Retriever must call `LlmGateway::embed` with a Turn LLM scope and shared limiter; this spec adds no such call.

### 4.3 Observability

Required structured spans:

```text
context.prepare {
    story_id,
    turn_id,
    base_revision,
    recent_segment_count,
    scene_character_count,
    entity_signal_count,
    topic_signal_count,
    status,
    error_code
}

narrative.evaluate {
    story_id,
    turn_id,
    graph_revision,
    active_node_count,
    transition_count,
    impulse_count,
    status,
    error_code
}

context.retrieve {
    story_id,
    turn_id,
    base_revision,
    request_count,
    provider_count,
    candidate_count,
    writer_item_count,
    character_audience_count,
    character_item_count,
    total_token_cost,
    zero_result_request_count,
    status,
    error_code
}

llm.call {
    story_id,
    turn_id,
    stage,
    purpose,
    profile,
    provider,
    model,
    status
}
```

Identifiers and error codes are structured fields, not interpolated messages. Spans/logs never contain Player Input, Story/Summary text, Character profile, Memory, Rumor, Fact, Prompt, query text, reason text, or LLM response under the default metadata-only policy. The existing development-only bounded redacted-content policy remains the sole exception.

---

## 5. Acceptance Criteria

### 5.1 Domain and Snapshot Contracts

- [ ] `cargo test -p aise --test story_continuity_tests` passes all cases in §3.21.
- [ ] `rg -n 'pub struct StoryReadSnapshot\b' crates/aise/src/domain --glob '*.rs'` returns exactly one match in `domain/story_instance/snapshot.rs`.
- [ ] `test ! -e crates/aise/src/domain/story_state.rs` succeeds.
- [ ] `rg -n 'domain::story_state|story_state::' crates --glob '*.rs'` returns zero matches.
- [ ] `rg -n '\bStoryConfig\b|story_instructions' crates/aise/src crates/aise-server/src --glob '*.rs'` returns zero matches.
- [ ] `rg -n '\bEntityRef\b' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `StorySummary` has `summarized_through: Option<StorySequence>` and `StoryTurn` has `sequence: StorySequence`.
- [ ] `CurrentScene` has stable Scene/Location keys and explicit present Character IDs.
- [ ] `ActiveStoryConstraint` matches §3.3 and `rg -n 'active_constraints: Vec<String>|active_constraints.*map\(' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] World Book Topic/alias collision, missing Topic, limit, and Pack `Character` Entity tests pass.
- [ ] Runtime Fact, Rumor, and Memory contracts contain all shared metadata from §3.4.

### 5.2 Baseline, Planning, and Prompt Contracts

- [ ] `cargo test -p aise --test context_preparation_retrieval_tests` passes the Baseline, signals, planning, retrieval, isolation, ranking, and stage cases in §3.21.
- [ ] Baseline serialized-field snapshot equals §3.6 and contains no Player Input, Prompt, Fact, Rumor, or Memory body.
- [ ] Topic matching tests cover English case/boundaries, whitespace normalization, Chinese aliases, longest alias ordering, collision rejection, and stable Topic-key ties.
- [ ] `WriterPlanner` invokes Narrative evaluation once and one Planner completion once.
- [ ] Strict Planner deserialization rejects every forbidden field from §3.9.
- [ ] Planner-output permutations yield byte-identical canonical `RetrievalPlan` JSON.
- [ ] `rg -n 'struct ContextRequest\b|enum ContextSource\b|retrieval_requests:' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `rg -n 'use_tag_search|use_bm25|use_embedding|\btop_k\b|\bretriever:' crates/aise/src/domain crates/aise/src/planning --glob '*.rs'` returns zero matches.
- [ ] `test ! -e crates/aise/src/prompt/context_merger.rs` and `test ! -e crates/aise/src/prompt/tests/context_merger_tests.rs` both succeed.
- [ ] `test ! -e crates/aise/src/context/context_item.rs` succeeds.
- [ ] `rg -n 'ContextMerger|GenerationInput|CompletionSpec|ChatMessage|Role::System|system_message\(' crates/aise/src/context crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/src/validation --glob '*.rs'` returns zero matches.
- [ ] `cargo test -p aise --test prompt_context_contract_tests` passes for all five profiles and trust-boundary adversarial inputs.
- [ ] Every business Pipeline calls `complete_typed`; raw `LlmGateway::complete` has no business-Pipeline call site.

### 5.3 Retrieval and Persistence Contracts

- [ ] `cargo test -p aise --test knowledge_read_port_tests` passes authorization, scope, bound, and index-use cases from §3.21.
- [ ] Production composition registers exactly Entity and Topic Candidate Retrievers.
- [ ] `rg -n 'struct (Bm25CandidateRetriever|EmbeddingCandidateRetriever)\b' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `CandidateRetrieverKind` contains reserved `Bm25` and `Embedding` variants.
- [ ] Character Fact and cross-owner Memory tests prove the Store lookup count is zero when authorization fails.
- [ ] Entity+Topic duplicate, cross-kind conflict, stable ranking, and round-robin trimming tests match §§3.11–3.13 exactly.
- [ ] Zero-result tests prove no full-scan fallback and no unrelated Context item.
- [ ] Revision and Pack digest mismatch tests return no item and the exact conflict path.
- [ ] `0009_context_retrieval.sql` adds Story sequence and all three indexed knowledge structures from §3.18.
- [ ] `EXPLAIN QUERY PLAN` tests identify Entity/Topic mapping indexes for production Candidate queries.
- [ ] `rg -n 'facts_json|rumors_json|memories_json' crates/aise/src --glob '*.rs'` returns zero production matches.
- [ ] `rg -n 'split_whitespace\(\).*to_lowercase|keyword_score|collect_source' crates/aise/src/context --glob '*.rs'` returns zero matches.
- [ ] Snapshot loading performs no Fact/Rumor/Memory body query; Candidate retrieval performs no World Book/knowledge full scan.
- [ ] Story commit assigns `N+1`, the database rejects a duplicate `(world_id, sequence)`, and a rejected commit leaves no partial sequence or knowledge write.

### 5.4 Runtime, Bounds, and Static Removal

- [ ] `TurnExecutionContext.retrieved` is `RetrievedContext`, not `Vec<ContextItem>`.
- [ ] `requires_retrieval` and `requires_character_thinking` derive from final request collections.
- [ ] `rg -n 'TODO\(temp-debug\)|temporarily disabled while debugging' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] Empty Retrieval/Character request integration cases skip the respective Pipeline; non-empty cases invoke it exactly once.
- [ ] Generator Context includes Writer items and Thoughts but no raw Character partitions.
- [ ] `TurnExecutionContext::set_retrieved_context` rejects every per-audience and total count/byte/token overflow.
- [ ] Every new config numeric zero is rejected, and every cross-field relation in §3.17 has a negative test.
- [ ] `rg -n 'max_story_instructions_bytes|max_story_config_bytes|max_world_fact_bytes|max_memories|max_memory_bytes' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `rg -n '\bmax_retrieved_items\b|\bmax_retrieval_candidates\b|\bmax_retrieved_item_bytes\b|\bmax_retrieved_tokens\b' crates/aise/src/config.rs` returns zero matches.
- [ ] `rg -n '\bmax_recent_turns\b|\bmax_recent_turn_bytes\b' crates/aise/src/config.rs` returns zero matches.
- [ ] `rg -n 'pub fn estimate_text_tokens\b' crates/aise/src --glob '*.rs'` returns exactly one match in `domain/text/token_estimator.rs`, and `rg -n 'pub fn estimate_tokens\(text' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `cargo test -p aise --test dependency_direction_tests` passes.
- [ ] `rg -n -U '(?:crate|super(?:::\s*super)*)::\s*turn\b|(?:crate|super)::\s*\{[^}]*\bturn(?:::|,|\})' crates/aise/src/domain --glob '*.rs'` returns zero matches.
- [ ] `domain/turn/mod.rs` and every touched `mod.rs`/`lib.rs` remain index-only.
- [ ] All new unit tests use dedicated `tests/<source>_tests.rs`; no new inline `mod tests` block exists.
- [ ] `doc/design/2026-08-04-Architecture-gpt.md` §§8.2–8.3 and §§10–11 describe the final contracts and no longer list legacy Baseline fields.

### 5.5 End-to-End and Toolchain

- [ ] `context_retrieval_end_to_end_is_revision_consistent` passes with Automatic-only retrieval, Planner-gap retrieval, Character isolation, generation, validation, and commit.
- [ ] Existing Story Pack, Narrative Graph, Runtime, Prompt trust-boundary, persistence, SSE, and Turn API tests are migrated and pass without a legacy compatibility fixture.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Implement BM25 only after observed zero-result and relevance metrics justify it; add a concrete `CandidateRetriever` and lexical index under a separate spec.
- Implement Embedding only after the same evidence; all calls must use `LlmGateway::embed`, shared limits, and a rebuildable `EntryId + ContentHash` index.
- Add multi-provider ranking fusion such as RRF only when at least two ranked non-exact providers exist; never add raw provider scores directly.
- Define Summary model selection, compaction triggers, background execution, and recovery under a separate Summary lifecycle design/spec.
- Add recursive Entry activation only with an explicit depth, work, token, and cycle budget under a separate design.
- Add multiplayer knowledge audiences and per-player private Context under a separate multiplayer contract.

---

## 7. References

- Source design: [Context Preparation and Retrieval — Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Turn Runtime architecture: [AISE Technical Architecture v3.1](../design/2026-08-04-Architecture-gpt.md)
- Story Pack trust and knowledge model: [AISE Story Pack Design v3.0](../design/2026-08-06-StoryPackDesign-gpt.md)
- Prior Story Pack implementation contract: [Story Pack v3 — Spec](./2026-08-07-story-pack-v3-spec-gpt.md)
- Earlier temporary two-Snapshot rule superseded by this spec: [Domain-to-Core Dependency Removal — Spec](./2026-08-08-domain-core-dependency-removal-spec-gpt.md)
- Current legacy Baseline and flat Context contracts: `crates/aise/src/turn/turn_data.rs:103`, `crates/aise/src/turn/turn_data.rs:146`
- Current Baseline copies old instructions/config/all Characters: `crates/aise/src/context/baseline_ctx_builder.rs:53`
- Current full-scan keyword Retrieval: `crates/aise/src/context/retrieval_pipeline.rs:20`, `crates/aise/src/context/retrieval_pipeline.rs:109`
- Current unconditional Retrieval/Character skips: `crates/aise/src/turn/turn_context.rs:295`
- Current duplicate scoped Context model: `crates/aise/src/context/context_item.rs:6`
- Current Story Pack v3 Snapshot with eager knowledge bodies: `crates/aise/src/domain/story_instance/snapshot.rs:30`
- Current World Book Entry metadata: `crates/aise/src/domain/asset/world_book.rs:33`
- Current incomplete runtime Fact/Rumor/Memory metadata: `crates/aise/src/domain/knowledge/fact.rs:15`, `crates/aise/src/domain/knowledge/rumor.rs:8`, `crates/aise/src/domain/knowledge/memory.rs:8`
- Current content-built Prompt messages: `crates/aise/src/prompt/context_merger.rs:42`, `crates/aise/src/prompt/context_merger.rs:102`
- Existing typed Prompt boundary to complete: `crates/aise/src/prompt/model_request.rs:11`, `crates/aise/src/llm/gateway.rs:56`
- Guardrails: [Architecture and Refactor](../agents/guardrails/architecture-refactor.md), [Layer Dependencies](../agents/guardrails/layer-dependencies.md), [Concurrency](../agents/guardrails/concurrency.md), [Code Organization](../agents/guardrails/code-organization.md), [Errors and Observability](../agents/guardrails/observability.md), [Toolchain](../agents/guardrails/toolchain.md)
