# Role and Context Runtime Decoupling Phase 0 — Spec
> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-17
> **Status**: Proposed
> **Source Design**: [Seed 与 Runtime 角色及故事上下文解耦](../../design/2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)
> **Phase**: Phase 0

---

## 1. Goal

Replace mixed seed/runtime role state with a single bounded runtime role aggregate, freeze character-card overrides and opening text at story creation, and convert Turn-stage role transport to ID-only contracts.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Replace `RoleBody` with `RoleSpec`, `SeedRole`, and the final runtime `RoleEntry`.
- Freeze `CharacterCard.profile`, seed memories, and opening text into the initial `StoryContext`.
- Add shared role-count, role-catalog, role-state, per-role-memory, alias-count, and opening-size boundaries.
- Replace `StoryAssetSpec` with a reference-safe `StoryPackSnapshot` and `StoryCreateBundle`.
- Keep `StoryInstanceSpec` as a crate-private Turn-0 creation command and persist `player_id` in `StoryInstanceInfo`.
- Make story creation compensating and story removal idempotently retryable across `StoryStore` and `PackStore`.
- Remove runtime character-card storage and lookup.
- Remove `RoleActor`, `CharacterCardRef`, `ResolvedRole`, and `PlayerRoleContext`.
- Convert `plan`, `retrieval`, `think`, `generate`, `validate`, `repair`, and `summary` to runtime role lookup by `RoleId`.
- Remove `Arc<Store>` from those seven stages; keep it in `baseline`, `extract`, and `commit`.
- Preserve the external `StoryPack` JSON shape, canonical field order, defaults, unknown-field rejection, and `PackRef.digest`.
- Hard-rename `ThinkConfig.max_seed_memories` to `max_memories_in_prompt`.

### 2.2 Non-Goals

- Does not add, update, delete, or switch the player assignment of runtime roles during a Turn.
- Does not change fact, rumor, narrative, summary, or recent-Turn capacity policies.
- Does not remove `Store` from `baseline` or `extract`; both still read `NarrativeGraph` and pack constraints.
- Does not change the external `StoryPack` schema or introduce old internal-state compatibility readers.
- Does not modify `crates/aise` or `crates/aise-server`.
- Does not add speculative metrics, trace metadata, or prompt redaction infrastructure.

### 2.3 Implementation Constraints

- This is a hard refactor under `R-REFACTOR-01` and `R-REFACTOR-02`.
- Do not retain fallback paths, serde aliases, compatibility shims, adapters, dual writes, or deprecated old types.
- Delete superseded code, configuration fields, tests, fixtures, and prompt variables in the same change.
- `core` must remain independent of `persistence`, `pipeline`, and `engine`.
- Pipeline stages must not import one another; only `pipeline::runtime` may compose stage contracts.
- Keep `mod.rs` and `lib.rs` index-only, put unit tests in `tests/<source>_tests.rs`, keep imports contiguous, and add no code comments except existing waiver markers required by project policy.
- No new dependency is permitted.
- Existing waivers for unbounded non-role story state may remain only at their exact surviving boundary; remove role/opening coverage from broad waivers.

---

## 3. Contracts

### 3.1 Role Types and Limits

Define these contracts in `crates/aise-core/src/core/role/role.rs` and re-export them through the existing index modules:

```rust
pub const MAX_ROLES_PER_STORY: usize = 64;
pub const MAX_ALIASES_PER_ROLE: usize = 8;
pub const MAX_ROLE_CATALOG_BYTES: usize = 32 * 1024;
pub const MAX_MEMORIES_PER_ROLE: usize = 32;
pub const MAX_ROLE_STATE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OPENING_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    pub name: BoundedText,
    #[serde(default)]
    pub alias: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval: RetrievalHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedRole {
    pub spec: RoleSpec,
    pub default_profile: CharacterProfile,
    pub seed_memories: BTreeMap<MemoryId, MemoryBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub id: RoleId,
    pub origin: EntryOrigin,
    pub spec: RoleSpec,
    pub profile: CharacterProfile,
    pub memories: BTreeMap<MemoryId, MemoryEntry>,
}
```

`SeedRole` must use custom serde so its external representation remains flat and byte-for-byte ordered as:

```json
{
  "name": "bounded text",
  "alias": [],
  "backstory": "bounded text",
  "retrieval": "retrieval hint",
  "default_profile": {},
  "seed_memories": {}
}
```

The deserializer must reject unknown and duplicate fields. Missing `alias` and `seed_memories` must deserialize as empty collections.

Expose shared validation from the owning `core::role` module:

```rust
pub(crate) fn validate_role_spec(role_id: &RoleId, spec: &RoleSpec) -> Result<(), StoryError>;

pub(crate) fn validate_seed_roles(
    roles: &BTreeMap<RoleId, SeedRole>,
) -> Result<(), StoryError>;

pub(crate) fn validate_runtime_roles(
    roles: &BTreeMap<RoleId, RoleEntry>,
) -> Result<(), StoryError>;
```

`validate_seed_roles` must project each seed role to the runtime serialization shape with `EntryOrigin::Seed` and seed `MemoryEntry` values before applying `MAX_ROLE_STATE_BYTES`.

### 3.2 Story Types

Update `StoryPack` and runtime story contracts as follows:

```rust
pub struct StoryPack {
    pub spec: StoryPackSpec,
    pub pack_id: PackId,
    pub meta: StoryPackMeta,
    pub profile: StoryPackProfile,
    pub roles: BTreeMap<RoleId, SeedRole>,
    pub play: PlayDefinition,
    pub opening: BoundedText,
    pub seed_facts: BTreeMap<FactId, FactBody>,
    pub seed_rumors: BTreeMap<RumorId, RumorBody>,
    pub constraints: Vec<BoundedText>,
    pub narrative: Option<NarrativeGraph>,
}

pub struct StoryContext {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub opening: Arc<BoundedText>,
    pub roles: BTreeMap<RoleId, RoleEntry>,
    pub facts: BTreeMap<FactId, FactEntry>,
    pub rumors: BTreeMap<RumorId, RumorEntry>,
    pub player: RoleId,
    pub turn_number: TurnNumber,
    pub summary: Option<StorySummary>,
    pub recent_turns: VecDeque<Turn>,
    pub life_cycle: StoryLifeCycle,
    pub narrative: NarrativeState,
}

pub struct StoryInstanceInfo {
    pub story_id: StoryId,
    pub player_id: PlayerId,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub life_cycle: StoryLifeCycle,
}
```

Delete `StoryContext::new()` and `impl Default for StoryContext`. Test construction belongs in `crates/aise-core/src/core/tests/fixture.rs`.

### 3.3 Creation and Persistence Contracts

Define crate-private ownership-safe creation contracts in `core/story/story_instance.rs`:

```rust
pub(crate) struct StoryPackSnapshot {
    pack_ref: PackRef,
    pack: StoryPack,
}

impl StoryPackSnapshot {
    pub(crate) fn from_pack(pack: StoryPack) -> Result<Self, CoreError>;
    pub(crate) fn pack_ref(&self) -> &PackRef;
    pub(crate) fn into_parts(self) -> (PackRef, StoryPack);
}

pub(crate) struct StoryInstanceSpec {
    story_id: StoryId,
    pack_ref: PackRef,
    opening: Arc<BoundedText>,
    roles: BTreeMap<RoleId, RoleEntry>,
    facts: BTreeMap<FactId, FactEntry>,
    rumors: BTreeMap<RumorId, RumorEntry>,
    player_id: PlayerId,
    player_role: RoleId,
    narrative: NarrativeState,
}

impl StoryInstanceSpec {
    pub(crate) fn into_parts(self) -> StoryInstanceParts;
}

pub(crate) struct StoryInstanceParts {
    pub(crate) story_id: StoryId,
    pub(crate) pack_ref: PackRef,
    pub(crate) opening: Arc<BoundedText>,
    pub(crate) roles: BTreeMap<RoleId, RoleEntry>,
    pub(crate) facts: BTreeMap<FactId, FactEntry>,
    pub(crate) rumors: BTreeMap<RumorId, RumorEntry>,
    pub(crate) player_id: PlayerId,
    pub(crate) player_role: RoleId,
    pub(crate) narrative: NarrativeState,
}

pub(crate) struct StoryCreateBundle {
    snapshot: StoryPackSnapshot,
    instance: StoryInstanceSpec,
}

impl StoryCreateBundle {
    pub(crate) fn assemble(
        snapshot: StoryPackSnapshot,
        story_id: StoryId,
        opening: Arc<BoundedText>,
        roles: BTreeMap<RoleId, RoleEntry>,
        facts: BTreeMap<FactId, FactEntry>,
        rumors: BTreeMap<RumorId, RumorEntry>,
        player_id: PlayerId,
        player_role: RoleId,
        narrative: NarrativeState,
    ) -> Self;

    pub(crate) fn into_parts(self) -> (StoryPackSnapshot, StoryInstanceSpec);
}
```

`StoryCreateBundle::assemble` must derive `StoryInstanceSpec.pack_ref` by cloning `snapshot.pack_ref()`; it must not accept a caller-supplied `PackRef`.

Make raw persistence traits, memory/SQLite backends, and raw backend accessors crate-private:

```rust
#[async_trait]
pub(crate) trait PackStore: Send + Sync {
    async fn retain(
        &self,
        story_id: &StoryId,
        snapshot: StoryPackSnapshot,
    ) -> Result<(), PersistenceError>;
    async fn release(&self, story_id: &StoryId) -> Result<(), PersistenceError>;
    async fn get_pack(&self, pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError>;
}

#[async_trait]
pub(crate) trait StoryStore: Send + Sync {
    async fn create(&self, spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn remove(&self, story_id: &StoryId) -> Result<(), PersistenceError>;
    async fn get_info(&self, story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError>;
    async fn load(&self, story_id: &StoryId) -> Result<StoryContext, PersistenceError>;
    async fn find_committed(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError>;
    async fn commit(&self, commit: StoryCommit) -> Result<StoryCommit, PersistenceError>;
}

impl Store {
    pub(crate) async fn create_story(
        &self,
        bundle: StoryCreateBundle,
    ) -> Result<StoryInstanceInfo, PersistenceError>;

    pub(crate) async fn remove_story(
        &self,
        story_id: &StoryId,
    ) -> Result<(), PersistenceError>;

    pub(crate) fn story_store(&self) -> &dyn StoryStore;
    pub(crate) fn pack_store(&self) -> &dyn PackStore;
}
```

Delete `PackStore::get_character_card`, all character-card cache state, and `PackStoreConfig.max_character_cards`.

Add the typed compensation error:

```rust
#[error(
    "story creation and pack cleanup failed for {story_id}: create={create_source}; cleanup={cleanup_source}"
)]
CreateCompensationFailed {
    story_id: StoryId,
    create_source: Box<PersistenceError>,
    cleanup_source: Box<PersistenceError>,
}
```

### 3.4 Factory Contracts

Keep the public creation input, but consume character cards only during freezing:

```rust
pub struct StoryCreationSpec {
    pub story_pack: StoryPack,
    pub cast: BTreeMap<RoleId, RoleActorSpec>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
}

pub enum RoleActorSpec {
    Default,
    Card(Box<CharacterCard>),
}

impl AiseFactory {
    pub async fn create_story(
        &self,
        store: Arc<Store>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError>;

    pub async fn remove_story(
        &self,
        store: Arc<Store>,
        story_id: &StoryId,
    ) -> Result<(), EngineError>;
}
```

Delete `FrozenStory`, `FrozenCast`, `freeze_cast`, `release_story_assets`, `RoleActor`, `CharacterCardRef`, and `CharacterCard::card_ref`.

### 3.5 Pipeline Data Contracts

Use these final stage data shapes:

```rust
pub struct PlanOutput {
    pub story_goal: BoundedText,
    pub facts_to_retrieve: Vec<FactId>,
    pub rumors_to_retrieve: Vec<RumorId>,
    pub roles_for_generation: Vec<RoleId>,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        !self.facts_to_retrieve.is_empty() || !self.rumors_to_retrieve.is_empty()
    }
}

pub struct RetrievalInput<'a> {
    pub fact_ids: &'a [FactId],
    pub rumor_ids: &'a [RumorId],
}

pub struct RetrievalOutput {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
}

pub struct ThinkInput<'a> {
    pub requests: &'a [CharacterThinkRequest],
    pub player_contribution: &'a PlayerContribution,
    pub activated_rumors: &'a [RumorEntry],
    pub retrieved_rumors: &'a [RumorEntry],
}

pub struct GenerateInput<'a> {
    pub story_goal: &'a str,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub role_ids: &'a [RoleId],
    pub character_decisions: &'a [CharacterDecision],
}

pub struct ValidationContext<'a> {
    pub story_goal: &'a BoundedText,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub role_ids: &'a [RoleId],
    pub character_decisions: &'a [CharacterDecision],
}
```

`GenerateInput.role_ids` and `ValidationContext.role_ids` contain the stable de-duplicated union of `roles_for_generation` followed by Character Think request role IDs.

Constructor contracts after store removal:

```rust
PlanPipeline::new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, config: PlanConfig)
RetrievalPipeline::new()
ThinkPipeline::new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, config: ThinkConfig)
GeneratePipeline::new(
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: GenerateConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
)
ValidatePipeline::new(
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: ValidateConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
)
RepairPipeline::new(
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    config: RepairConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
)
SummaryPipeline::new(gateway: Arc<LlmGateway>, prompt: Arc<Prompt>, config: SummaryConfig)
```

Delete `ResolvedRole` and `PlayerRoleContext`. Prompt projectors must borrow `RoleEntry` values from `&StoryContext` and serialize only the fields required by that prompt.

### 3.6 Opening and Think Configuration

Replace asynchronous pack loading with:

```rust
pub(in crate::pipeline) fn story_opening(story_ctx: &StoryContext) -> &str {
    match story_summary(story_ctx) {
        Some(_) => "",
        None => story_ctx.opening.as_str(),
    }
}

pub struct ThinkConfig {
    pub max_memories_in_prompt: usize,
}
```

The full existing `ThinkConfig` remains otherwise unchanged. Delete `load_story_opening`, the `max_seed_memories` field, its serde key, and prompt variable names containing `seed_memories`; use `target_memories`.

### 3.7 File Layout

Add or change files under the existing classic module layout:

```text
crates/aise-core/src/
├── core/
│   ├── role/
│   │   ├── role.rs
│   │   └── tests/role_tests.rs
│   ├── story/
│   │   ├── story_context.rs
│   │   ├── story_instance.rs
│   │   ├── story_pack.rs
│   │   └── tests/
│   └── tests/fixture.rs
├── engine/factory.rs
├── persistence/
│   ├── error.rs
│   ├── pack_store.rs
│   ├── pack_store_mem.rs
│   ├── pack_store_sqlite.rs
│   ├── story_store.rs
│   ├── story_store_mem.rs
│   ├── story_store_sqlite.rs
│   └── store.rs
└── pipeline/
    ├── baseline/
    ├── plan/
    ├── retrieval/
    ├── think/
    ├── generate/
    ├── validate/
    ├── repair/
    ├── extract/
    ├── summary/
    ├── common/
    └── runtime/
```

Delete:

```text
crates/aise-core/src/core/role/actor.rs
crates/aise-core/src/core/role/player_context.rs
```

Remove `ResolvedRole` from `core/role/character_think.rs`. Remove card-reference definitions from `core/role/character_card.rs`.

---

## 4. Behavior Rules

1. **P0-R1 Seed/runtime separation**: `StoryPack.roles` contains only `SeedRole`; `StoryContext.roles` contains only `RoleEntry`; no runtime type may contain `default_profile`, `seed_memories`, `RoleActor`, or `CharacterCardRef`.
2. **P0-R2 Card override**: `RoleActorSpec::Card(card)` replaces only `SeedRole.default_profile`; `RoleSpec` and seed memories remain unchanged. `RoleActorSpec::Default` and an omitted cast entry use `default_profile`.
3. **P0-R3 Origin conversion**: Every initial role, fact, rumor, and role memory is stored with `EntryOrigin::Seed`.
4. **P0-R4 Cast validation**: The player role must be playable, every cast key must exist in `StoryPack.roles`, and every runtime role must be created exactly once.
5. **P0-R5 Opening ownership**: `freeze_story` rejects `opening.as_str().len() > MAX_OPENING_BYTES`; accepted opening text is placed in `Arc<BoundedText>` and cloned by reference in context snapshots.
6. **P0-R6 Shared role limits**: Pack validation, story creation, and every future role-state validation use the same core constants and validators. No persistence or pipeline config may duplicate those values.
7. **P0-R7 Catalog formula**: Catalog bytes are the saturating sum of UTF-8 byte lengths for role ID, name, each alias, and retrieval text. JSON punctuation, backstory, profile, and memories are excluded.
8. **P0-R8 State-size measurement**: Role-state bytes are measured through a counting writer that aborts as soon as `MAX_ROLE_STATE_BYTES` is exceeded. Do not allocate a full `serde_json::to_vec` buffer for size detection.
9. **P0-R9 No partial capacity handling**: Capacity violations reject the complete create operation. Do not evict, truncate, or partially insert roles or memories.
10. **P0-R10 Pack identity**: `StoryPackSnapshot::from_pack` is the only constructor binding a `PackRef` to a pack. `StoryCreateBundle::assemble` must copy that exact reference into its private instance spec.
11. **P0-R11 Unique holder**: `PackStore::retain` checks capacity and rejects an already-bound `StoryId` before mutating any holder or reference count. One story can hold exactly one snapshot.
12. **P0-R12 Turn-0 creation**: `StoryStore::create` constructs `turn_number = 0`, `summary = None`, empty `recent_turns`, and `life_cycle = Active`; it stores `player_id` in `StoryInstanceInfo`.
13. **P0-R13 Creation compensation**: `Store::create_story` retains the snapshot before creating the story. If create fails, it releases the snapshot. If release succeeds, return the original create error. If release fails, return `CreateCompensationFailed` with the story ID and both boxed sources.
14. **P0-R14 Idempotent removal**: `Store::remove_story` removes the story first and releases the pack second. Missing story and missing holder are success. A non-`NotFound` story error stops before release. A release error remains retryable by calling the same method again.
15. **P0-R15 PackStore scope**: `PackStore` stores only pack snapshots and story holders; it has no character-card map, card capacity, or card lookup.
16. **P0-R16 Retrieval trigger**: Runtime invokes retrieval only when facts or rumors were requested. Plans containing only generation roles or Character Think requests must skip retrieval.
17. **P0-R17 Effective role order**: Runtime creates `effective_role_ids` by taking `roles_for_generation` in order, then Character Think role IDs in request order, retaining the first occurrence of each ID.
18. **P0-R18 Snapshot lookup**: Think, generate, validate, and repair resolve every requested role and the player role from the same read-only `StoryContext`. Missing IDs return `PipelineError::InconsistentData` with the owning stage and a stable code; they are never skipped.
19. **P0-R19 Role privacy**: Stage inputs and outputs may contain role IDs but not full `RoleEntry` values or memory maps. Generate, validate, and repair prompt projections exclude memories. Think may include only the target role memories truncated by `max_memories_in_prompt`.
20. **P0-R20 Opening fallback**: When a non-empty summary exists, `story_opening` returns `""`; otherwise it returns the frozen opening synchronously.
21. **P0-R21 Store dependencies**: `plan`, `retrieval`, `think`, `generate`, `validate`, `repair`, and `summary` contain no `Store` field and perform no storage call. `baseline` and `extract` retain Store only for pack narrative/constraints. `commit` remains the only writer.
22. **P0-R22 External pack stability**: For every accepted legacy story-pack fixture, the new `SeedRole` serialization bytes and canonical `PackRef.digest` equal the pre-refactor golden values.
23. **P0-R23 Hard configuration rename**: Deserializing `max_seed_memories` fails as an unknown field; `max_memories_in_prompt` is the only accepted key.
24. **P0-R24 Context fixture ownership**: Production code exposes no fake/default `StoryContext`; unit tests use fixture builders outside production source files.

### 4.1 Error Handling

- Role boundary failures return typed `StoryError` variants carrying `actual`, `maximum`, and `role_id` where applicable.
- Persistence capacity, duplicate holder, invalid reference, and atomicity failures return typed `PersistenceError`; no `anyhow::Error` may escape `aise-core`.
- Pipeline snapshot inconsistencies return `PipelineError::InconsistentData` with stage, stable code, and the missing identifier in the message.
- Do not use `unwrap` or `expect` for story-pack, cast, LLM, persistence, or serialized input.
- Preserve `EngineError::Persistence` so `CreateCompensationFailed.story_id` remains available to external callers.

### 4.2 Concurrency

- `PackStore::retain` must validate and insert the holder within one write-lock acquisition.
- No lock guard may cross `.await`, emit an event, write a log, or perform I/O.
- Context cloning must clone the opening `Arc`, not the underlying text.
- This phase adds no task, queue, channel, or concurrency fan-out.

### 4.3 Observability

- Keep the existing `PipelineRunner` stage observations and explicit parent observations.
- Stage input/output captures must contain role IDs only; they must not serialize existing role memory maps.
- Think prompt observation may capture only the actual bounded target memories sent to the LLM.
- Log create-compensation and pack-release failures with structured `story_id` and `error` fields.
- Add no metric or metadata field without a current consumer and verified export path.

---

## 5. Acceptance Criteria

- [ ] `RoleBody`, `RoleActor`, `CharacterCardRef`, `ResolvedRole`, `PlayerRoleContext`, `StoryAssetSpec`, `FrozenStory`, and `FrozenCast` have zero matches under `crates/aise-core/src`.
- [ ] `StoryPack.roles` is `BTreeMap<RoleId, SeedRole>` and `StoryContext.roles` is `BTreeMap<RoleId, RoleEntry>`.
- [ ] `RoleEntry` has exactly `id`, `origin`, `spec`, `profile`, and `memories`.
- [ ] `StoryContext` has `opening: Arc<BoundedText>` and has no `new` or `Default` implementation.
- [ ] `StoryInstanceInfo` persists `player_id`.
- [ ] `PackStore` has no character-card API or state; `PackStoreConfig` rejects `max_character_cards`.
- [ ] `plan`, `retrieval`, `think`, `generate`, `validate`, `repair`, and `summary` contain no `Arc<Store>` field.
- [ ] `PlanOutput::requires_retrieval` checks only fact and rumor IDs.
- [ ] `RetrievalInput` and `RetrievalOutput` contain no role field.
- [ ] Stage input/output serialization contains no existing role memory sentinel outside the selected Think prompt.
- [ ] `pipeline_prompt::load_story_opening` has zero matches and all prior-story fallback tests use frozen context opening.
- [ ] `ThinkConfig` accepts `max_memories_in_prompt` and rejects `max_seed_memories`.
- [ ] Legacy `StoryPack` round-trip, unknown-field, duplicate-field, serialized-byte, and golden-digest tests pass.
- [ ] Card profile override tests prove spec identity and seed memories are unchanged.
- [ ] Role boundary tests cover exact limits and one-over limits for count, aliases, catalog bytes, per-role memories, total state bytes, and opening bytes.
- [ ] The role-state size test proves counting stops at the limit without a full temporary serialization buffer.
- [ ] Duplicate story holder insertion leaves the original holder and reference count unchanged.
- [ ] Create-failure/release-success returns the original error; create-failure/release-failure returns both sources and `story_id`.
- [ ] Store-level and Engine-level repeated removal completes after a simulated first release failure.
- [ ] Generation-only and Think-only plans execute retrieval zero times.
- [ ] `effective_role_ids` tests prove stable first-occurrence ordering for overlap and duplicates.
- [ ] Missing role tests for Think, Generate, Validate, and Repair assert stage and stable error code.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test -p aise-core` passes.
- [ ] `cargo clippy -p aise-core --all-targets -- -D warnings` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Runtime role creation and initial-memory extraction are specified in [Phase 1](./2026-10-17-role-context-runtime-decoupling-spec-phase-1-gpt.md).
- Runtime role update, role deletion, and player-role reassignment remain out of scope.
- Production SQLite migration remains out of scope because the current SQLite backend delegates to in-memory storage.

---

## 7. References

- Source design: [Seed 与 Runtime 角色及故事上下文解耦](../../design/2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)
- Architecture baseline: [Pipeline 框架与编排](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Architecture and refactor guardrails: [architecture-refactor.md](../../agents/guardrails/architecture-refactor.md)
- Layer guardrails: [layer-dependencies.md](../../agents/guardrails/layer-dependencies.md)
- Pipeline guardrails: [pipeline.md](../../agents/guardrails/pipeline.md)
- Code organization guardrails: [code-organization.md](../../agents/guardrails/code-organization.md)
- Observability guardrails: [observability.md](../../agents/guardrails/observability.md)
