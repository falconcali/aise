# Extract Pipeline and Dynamic Knowledge Store — Spec (Phase 0: StoryContext, KnowledgeChange, and Store Wiring)

> **Model**: Gemini 4 Argon
> **Date**: 2026-10-17
> **Status**: Proposed
> **Source Design**: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
> **Phase**: Phase 0

---

## 1. Goal

Replace the placeholder `WorldState` and `WorldChange` structs with direct `facts`, `rumors`, and `roles` (`RoleEntry`) state on `StoryContext`, introduce `KnowledgeChange` operations, initialize seed knowledge and role memories on story creation, apply `KnowledgeChange` atomically in `StoryStore::commit`, and switch `baseline`, `plan`, `retrieval`, and `think` to read dynamic knowledge and role memories from `StoryContext`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete `crates/aise-core/src/core/world.rs` (`WorldState` and `WorldChange`) and remove their re-exports from `crates/aise-core/src/core/mod.rs`.
- Add `KnowledgeChange`, `FactChangeOp`, `RumorChangeOp`, and `RoleMemoryChangeOp` under `crates/aise-core/src/core/knowledge/` and re-export them from `crates/aise-core/src/core/mod.rs`.
- Replace `StoryContext.cast` and `StoryContext.world_state` with `roles: BTreeMap<RoleId, RoleEntry>`, `facts: BTreeMap<FactId, FactEntry>`, and `rumors: BTreeMap<RumorId, RumorEntry>`.
- Update `StoryInstanceSpec` to carry initial `roles: BTreeMap<RoleId, RoleEntry>`, `facts: BTreeMap<FactId, FactEntry>`, and `rumors: BTreeMap<RumorId, RumorEntry>` populated by `AiseFactory::freeze_story` from `StoryPack` seed data (`EntryOrigin::Seed`).
- Replace `Turn.world_change: WorldChange` and `CommitInput.world_change: WorldChange` with `knowledge_change: KnowledgeChange`.
- Update `ResolvedRole` (`seed_memories` -> `memories: BTreeMap<MemoryId, MemoryEntry>`) and `think` (`think_context` / `think_prompt` / `ThinkConfig`) to read full runtime role memories from `ResolvedRole.memories`.
- Update `StoryStoreMem::create` and `StoryStoreMem::commit` to initialize `roles`, `facts`, and `rumors`, validate `KnowledgeChange` invariants before mutating `next_context`, and atomically apply `FactChangeOp`, `RumorChangeOp`, and `RoleMemoryChangeOp`.
- Switch `baseline_knowledge`, `plan_candidates`, `plan_roles`, and `retrieval` to read `facts`, `rumors`, and `roles` directly from `StoryContext` instead of `StoryPack`.
- Remove unused `queries: Vec<SemanticQuery>` from `baseline_narrative::NarrativeProjection` and update `ExtractInput` to `{ pub proposal: &'a str }` and `ExtractOutput` to include `pub knowledge_change: KnowledgeChange`.

### 2.2 Non-Goals

- Does not add LLM calls or prompt templates to `ExtractPipeline` (handled in Phase 1 and Phase 2).
- Does not evaluate `SemanticQuery` via `LlmGateway` (handled in Phase 1).
- Does not extract `Fact`, `Rumor`, or `Memory` changes via `LlmGateway` (handled in Phase 2).
- Does not introduce structured scene/location state, character attribute key-value tables, or numeric relationship tables.
- Does not modify frozen reference crates (`crates/aise` and `crates/aise-server`).

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code for Phase 0 contracts. Do **not** keep fallback paths, compatibility shims, or dual-write logic (`R-REFACTOR-01`, `R-REFACTOR-02`).
- `WorldState`, `WorldChange`, `StoryContext.cast`, `StoryContext.world_state`, `Turn.world_change`, and `NarrativeProjection.queries` MUST be deleted, not deprecated.
- No mid-state "both `StoryPack` seed lookup and `StoryContext` lookup coexist in `baseline`/`plan`/`retrieval`" path is permitted.
- Keep type definitions and trait derives minimal and strictly necessary for production code: do **not** add test-only derives (such as `PartialEq`, `Eq`), speculative helper methods, or extra fields to `core` or `pipeline` types; tests MUST use `matches!`, field-level assertions, or `serde_json::to_value` comparisons instead of polluting production definitions.
- Rust source files MUST remain comment-free (`R-CODE-05`) and unit tests MUST live in `tests/<source>_tests.rs` (`R-CODE-02`).

---

## 3. Contracts

### 3.1 Knowledge Change Types

Create `crates/aise-core/src/core/knowledge/change.rs` and re-export from `core/knowledge/mod.rs` and `core/mod.rs`:

```rust
use crate::core::{FactBody, FactId, MemoryBody, MemoryId, RoleId, RumorBody, RumorId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeChange {
    #[serde(default)]
    pub facts: Vec<FactChangeOp>,
    #[serde(default)]
    pub rumors: Vec<RumorChangeOp>,
    #[serde(default)]
    pub memories: Vec<RoleMemoryChangeOp>,
}

impl KnowledgeChange {
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.rumors.is_empty() && self.memories.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactChangeOp {
    Add { id: FactId, body: FactBody },
    Update { id: FactId, body: FactBody },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RumorChangeOp {
    Add { id: RumorId, body: RumorBody },
    Update { id: RumorId, body: RumorBody },
    Remove { id: RumorId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleMemoryChangeOp {
    Add {
        role_id: RoleId,
        id: MemoryId,
        body: MemoryBody,
    },
    Update {
        role_id: RoleId,
        id: MemoryId,
        body: MemoryBody,
    },
}
```

### 3.2 `StoryContext`, `StoryInstanceSpec`, `Turn`, and `ResolvedRole`

Update `crates/aise-core/src/core/story/story_context.rs`:

```rust
use crate::core::{
    FactEntry, FactId, NarrativeState, PackId, PackRef, RoleEntry, RoleId, RumorEntry, RumorId, SemanticVersion,
    Sha256Digest, StoryId, StoryLifeCycle, Turn, TurnNumber,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryContext {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
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
```

Update `crates/aise-core/src/core/story/story_instance.rs`:

```rust
use crate::core::{
    FactEntry, FactId, NarrativeState, PackRef, PlayerId, RoleEntry, RoleId, RumorEntry, RumorId, StoryId,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct StoryInstanceSpec {
    pub story_id: StoryId,
    pub pack_ref: PackRef,
    pub roles: BTreeMap<RoleId, RoleEntry>,
    pub facts: BTreeMap<FactId, FactEntry>,
    pub rumors: BTreeMap<RumorId, RumorEntry>,
    pub player_id: PlayerId,
    pub player_role: RoleId,
    pub narrative: NarrativeState,
}
```

Update `Turn` in `crates/aise-core/src/core/turn.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub turn_number: TurnNumber,
    pub idempotency_key: IdempotencyKey,
    pub player_contribution: PlayerContribution,
    pub turn_segment: TurnSegment,
    pub knowledge_change: KnowledgeChange,
    pub turn_evaluation: TurnEvaluation,
    pub turn_status: TurnStatus,
}
```

Update `ResolvedRole` in `crates/aise-core/src/core/role/character_think.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedRole {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub aliases: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval_hint: RetrievalHint,
    pub profile: CharacterProfile,
    pub memories: BTreeMap<MemoryId, MemoryEntry>,
}
```

### 3.3 Upstream and Downstream Stage Contracts

Update `crates/aise-core/src/pipeline/baseline/baseline_narrative.rs`:

```rust
#[derive(Debug, Serialize)]
pub struct NarrativeProjection {
    pub directions: Vec<NarrativeDirection>,
    pub constraints: Vec<BoundedText>,
}
```

Update `crates/aise-core/src/pipeline/plan/plan_candidates.rs` and `plan_roles.rs`:

```rust
pub(super) fn collect_candidates<'a>(
    story_ctx: &'a StoryContext,
    activated: &ActivatedKnowledge,
    limit: usize,
) -> PlanCandidates<'a>;

pub(super) fn collect_role_catalog<'a>(
    story_ctx: &'a StoryContext,
    player_role: &RoleId,
) -> Vec<RoleCatalogEntry<'a>>;

pub(super) async fn resolve_player_role(
    store: &Store,
    story_ctx: &StoryContext,
) -> Result<PlayerRoleContext, PipelineError>;
```

Update `crates/aise-core/src/pipeline/extract/extract.rs` and `crates/aise-core/src/pipeline/commit/commit.rs`:

```rust
#[derive(Serialize)]
pub struct ExtractInput<'a> {
    pub proposal: &'a str,
}

#[derive(Serialize)]
pub struct ExtractOutput {
    pub knowledge_change: KnowledgeChange,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

#[derive(Serialize)]
pub struct CommitInput {
    pub player_contribution: PlayerContribution,
    pub idempotency_key: IdempotencyKey,
    pub turn_segment: TurnSegment,
    pub knowledge_change: KnowledgeChange,
    pub turn_status: TurnStatus,
    pub turn_evaluation: TurnEvaluation,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}
```

### 3.4 File / Directory Layout

```text
crates/aise-core/src/
├── core/
│   ├── knowledge/
│   │   ├── change.rs
│   │   ├── mod.rs
│   │   └── tests/change_tests.rs
│   ├── role/
│   │   ├── character_think.rs
│   │   └── role.rs
│   ├── story/
│   │   ├── story_context.rs
│   │   └── story_instance.rs
│   ├── mod.rs
│   └── turn.rs
├── engine/
│   └── factory.rs
├── persistence/
│   └── story_store_mem.rs
└── pipeline/
    ├── baseline/
    │   ├── baseline_knowledge.rs
    │   └── baseline_narrative.rs
    ├── commit/
    │   └── commit.rs
    ├── extract/
    │   └── extract.rs
    ├── plan/
    │   ├── plan.rs
    │   ├── plan_candidates.rs
    │   └── plan_roles.rs
    ├── retrieval/
    │   └── retrieval.rs
    ├── runtime/
    │   └── runtime.rs
    └── think/
        ├── think_context.rs
        └── think_prompt.rs
```

---

## 4. Behavior Rules

1. **R-P0-1 (Seed Initialization)**: `AiseFactory::freeze_story` MUST map `StoryPack.seed_facts` to `FactEntry { id, origin: EntryOrigin::Seed, body }`, `StoryPack.seed_rumors` to `RumorEntry { id, origin: EntryOrigin::Seed, body }`, and each role in `StoryPack.roles` + resolved `cast` to `RoleEntry { id, actor, body, memories }` where `memories` maps `body.seed_memories` to `MemoryEntry { id, origin: EntryOrigin::Seed, body }`.
2. **R-P0-2 (Authoritative Runtime Snapshot)**: `baseline_knowledge::activate_knowledge` MUST iterate `story_ctx.facts` and `story_ctx.rumors` (preserving each entry's existing `EntryOrigin`) and read the player role name from `story_ctx.roles.get(&story_ctx.player)`. `StoryPack` is still used only for `opening`.
3. **R-P0-3 (Plan Reads `StoryContext`)**: `plan_candidates::collect_candidates` MUST read unactivated `facts` and `rumors` from `story_ctx.facts` and `story_ctx.rumors`. `plan_roles::collect_role_catalog` and `plan_roles::resolve_player_role` MUST read `RoleEntry` from `story_ctx.roles`.
4. **R-P0-4 (Retrieval Reads `StoryContext`)**: `RetrievalPipeline` MUST resolve `fact_ids` from `story_ctx.facts`, `rumor_ids` from `story_ctx.rumors`, and `role_ids` from `story_ctx.roles` (populating `ResolvedRole.memories` with `role_entry.memories.clone()`), consulting `PackStore` only when `role_entry.actor` is `RoleActor::Card(card_ref)` to load `CharacterCard.profile`. `RetrievalPipeline` MUST NOT load `StoryPack`.
5. **R-P0-5 (Think Reads `ResolvedRole.memories`)**: `think_context` MUST iterate `role.memories.values()` (ordered by `MemoryId` in `BTreeMap`), rendering `memory.body.content.as_str()` under the existing memory count/byte bounds (`TBD`: final budget parameter names/limits if adjusted before acceptance).
6. **R-P0-6 (`StoryStore::commit` Pre-Mutation Validation)**: Before mutating `next_context`, `StoryStoreMem::commit` MUST validate `commit.turn.knowledge_change` in full:
   - Within `facts`: no duplicate `FactId` across operations; `Add` target `id` MUST NOT exist in `story.context.facts`; `Update` target `id` MUST exist in `story.context.facts`.
   - Within `rumors`: no duplicate `RumorId` across operations; `Add` target `id` MUST NOT exist in `story.context.rumors`; `Update` and `Remove` target `id` MUST exist in `story.context.rumors`.
   - Within `memories`: no duplicate `(role_id, id)` across operations; `role_id` MUST exist in `story.context.roles`; `Add` target `id` MUST NOT exist in `story.context.roles[&role_id].memories`; `Update` target `id` MUST exist in `story.context.roles[&role_id].memories`.
7. **R-P0-7 (`EntryOrigin` Preservation on Update)**: When `StoryStoreMem::commit` applies `KnowledgeChange` for Turn `T`:
   - `Add` operations insert an entry with `EntryOrigin::Turn(T)`.
   - `Update` operations replace `body` while preserving the existing entry's `origin` (`EntryOrigin::Seed` or `EntryOrigin::Turn(original_turn)`).
   - `RumorChangeOp::Remove` removes the `RumorId` from `next_context.rumors`.

### 4.1 Error Handling

- On any `KnowledgeChange` validation failure in `StoryStore::commit` (§4 R-P0-6), return `Err(PersistenceError::ConstraintViolation { constraint })` without modifying `story.context`, `story.info`, or `story.committed`.
- In `plan_roles::resolve_player_role`, if `story_ctx.player` is missing from `story_ctx.roles`, return `PipelineError::InconsistentData { stage: PipelineStage::Plan, code: "plan_player_role_missing", .. }`.
- In `RetrievalPipeline`, if a requested `FactId`, `RumorId`, or `RoleId` is missing from `story_ctx`, return `PipelineError::InconsistentData` with `stage: PipelineStage::Retrieval` and stable codes `retrieval_fact_missing`, `retrieval_rumor_missing`, or `retrieval_role_missing`.

### 4.2 Concurrency

- `StoryStoreMem::commit` validates and applies `knowledge_change`, `summary`, `narrative`, `life_cycle`, and `recent_turns` synchronously inside a single `stories.write().await` critical section with no `.await` or I/O while holding the lock (`R-CONC-01`, `R-CONC-03`).

### 4.3 Observability

- Existing stage spans in `baseline`, `plan`, `retrieval`, `think`, and `commit` remain under the explicitly passed parent `&Observation` (`R-OBS-02`).

---

## 5. Acceptance Criteria

- [ ] `crates/aise-core/src/core/world.rs` is deleted and `rg 'WorldState|WorldChange' crates/aise-core crates/aise-service` returns zero matches.
- [ ] `rg 'story_ctx\.cast|\.world_state|\.world_change' crates/aise-core crates/aise-service` returns zero matches.
- [ ] `StoryContext`, `StoryInstanceSpec`, `Turn`, `KnowledgeChange`, `FactChangeOp`, `RumorChangeOp`, and `RoleMemoryChangeOp` match §§3.1–3.2.
- [ ] `AiseFactory::create_story` initializes `StoryContext.facts`, `StoryContext.rumors`, and `StoryContext.roles` (with `memories` mapped from `RoleBody.seed_memories`) with `EntryOrigin::Seed`.
- [ ] `StoryStoreMem::commit` atomically applies valid `FactChangeOp`, `RumorChangeOp`, and `RoleMemoryChangeOp`, preserving `EntryOrigin` on `Update` and setting `EntryOrigin::Turn(turn_number)` on `Add`.
- [ ] `StoryStoreMem::commit` rejects duplicate IDs, `Add` of an existing ID, `Update`/`Remove` of a missing ID, or unknown `RoleId` with `PersistenceError::ConstraintViolation`, leaving `StoryContext` unchanged.
- [ ] `baseline`, `plan`, `retrieval`, and `think` unit tests verify that dynamically added/updated `FactEntry`, `RumorEntry`, and `MemoryEntry` in `StoryContext` are activated, listed as candidates, retrieved, and rendered into character thinking prompts, while removed `RumorEntry` items are absent.
- [ ] `cargo test -p aise-core` and `cargo clippy --workspace --all-targets -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- Narrative semantic query collection in `ExtractPipeline` and LLM evaluation of `SemanticQuery` → handled in [Phase 1](./2026-10-17-extract-pipeline-spec-phase-1-gemini.md).
- World knowledge (`Fact` / `Rumor`) and per-role memory (`Memory`) LLM extraction → handled in [Phase 2](./2026-10-17-extract-pipeline-spec-phase-2-gemini.md).
- Final capacity and byte budget values for cumulative entries and prompt/output limits remain `TBD` per source design until the dedicated budget pass.

---

## 7. References

- Source design: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
- Architecture baseline: [Pipeline Framework Design](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
