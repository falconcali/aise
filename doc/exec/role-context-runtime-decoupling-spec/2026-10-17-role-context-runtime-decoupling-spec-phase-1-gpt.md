# Dynamic Runtime Role Creation Phase 1 — Spec
> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-17
> **Status**: Proposed
> **Source Design**: [Seed 与 Runtime 角色及故事上下文解耦](../../design/2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)
> **Phase**: Phase 1

---

## 1. Goal

Add a bounded, deterministic, and atomic `extract → Runtime → commit` path that creates new runtime roles and their isolated initial memories without allowing the LLM to choose persisted IDs.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Extend world-knowledge extraction with new-role proposals containing `RoleSpec` and `CharacterProfile`.
- Allocate deterministic `RoleId` values in Rust after validating LLM output.
- Extract each new role's initial memories in an isolated per-role LLM call with empty prior memory.
- Share one bounded memory-extraction concurrency pool across existing and new roles.
- Add `RoleAddOp` to `core::role` and carry it separately through `ExtractOutput`, `CommitInput`, and `StoryCommit`.
- Allocate deterministic initial `MemoryId` values in Rust.
- Atomically apply role additions and existing knowledge changes in `StoryStore::commit`.
- Enforce per-Turn extraction budgets and the cumulative Phase 0 runtime-role limits.
- Update all three layers of `extract.extract_world_knowledge` and `extract.extract_role_memory`.

### 2.2 Non-Goals

- Does not update or delete an existing role's `RoleSpec` or `CharacterProfile`.
- Does not delete a role or change the player role.
- Does not let `roles_for_memory` reference a role created in the same Turn.
- Does not put role additions in `KnowledgeChange`, `Turn`, or `recent_turns`.
- Does not let world-knowledge extraction read or generate private role memories.
- Does not add a new prompt ID, pipeline stage, storage backend, ID type, or dependency.
- Does not modify `crates/aise` or `crates/aise-server`.

### 2.3 Implementation Constraints

- Phase 0 is a prerequisite; implement against `RoleSpec`, `SeedRole`, and the final runtime `RoleEntry`.
- This spec generates final-form code. Do not keep fallback paths, compatibility shims, dual response shapes, or deprecated fields.
- `RoleAddOp` belongs to `core::role::role_change`; it must not be placed in `core::knowledge::change`.
- `NewRoleProposal` and all LLM DTOs remain private to `pipeline::extract`.
- Only `pipeline::runtime` may move data between extract and commit stage contracts.
- Only the commit stage may write the complete `StoryCommit`.
- Use existing `LlmGateway`, prompt rendering, explicit-parent observations, and `PipelineRunner`.
- Keep `mod.rs` index-only, tests in dedicated files, imports contiguous, and code comment-free.
- No new dependency is permitted.

---

## 3. Contracts

### 3.1 Core Role-Addition Contract

Create `crates/aise-core/src/core/role/role_change.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleAddOp {
    pub id: RoleId,
    pub spec: RoleSpec,
    pub profile: CharacterProfile,
    pub initial_memories: BTreeMap<MemoryId, MemoryBody>,
}
```

Re-export `RoleAddOp` through `core::role` and `core`.

Update commit data:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryCommit {
    pub story_id: StoryId,
    pub turn: Turn,
    #[serde(default)]
    pub role_additions: Vec<RoleAddOp>,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}
```

Do not add `role_additions` to `Turn` or `KnowledgeChange`.

### 3.2 Extract Configuration

Extend the owning stage configuration:

```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractConfig {
    pub max_fact_changes: usize,
    pub max_rumor_changes: usize,
    pub max_roles_for_memory: usize,
    pub max_role_additions_per_turn: usize,
    pub max_memory_extractions_per_turn: usize,
    pub max_memory_changes_per_role: usize,
    pub max_memory_concurrency: NonZeroUsize,
}
```

Defaults:

```rust
max_role_additions_per_turn = 2
max_memory_extractions_per_turn = 4
```

Validation contract:

```rust
impl ExtractConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
}
```

`validate` must reject:

```text
max_role_additions_per_turn > max_memory_extractions_per_turn
max_memory_extractions_per_turn > MAX_ROLES_PER_STORY
max_roles_for_memory > max_memory_extractions_per_turn
max_memory_changes_per_role > MAX_MEMORIES_PER_ROLE
```

### 3.3 Extract Internal Types

Define these private stage types:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewRoleProposal {
    pub spec: RoleSpec,
    pub profile: CharacterProfile,
}

pub(super) struct AllocatedNewRole {
    pub id: RoleId,
    pub proposal: NewRoleProposal,
}

pub(super) struct WorldKnowledgeExtraction {
    pub facts: Vec<FactChangeOp>,
    pub rumors: Vec<RumorChangeOp>,
    pub roles_for_memory: Vec<RoleId>,
    pub new_roles: Vec<AllocatedNewRole>,
}

pub(super) enum RoleMemoryProposal {
    Add { body: MemoryBody },
    Update { id: MemoryId, body: MemoryBody },
}
```

The world-knowledge LLM response DTO must have this exact shape:

```json
{
  "facts": [],
  "rumors": [],
  "roles_for_memory": ["existing_role_id"],
  "new_roles": [
    {
      "spec": {
        "name": "bounded text",
        "alias": [],
        "backstory": "bounded text",
        "retrieval": "retrieval hint"
      },
      "profile": {}
    }
  ]
}
```

The LLM response contains no role ID, memory ID, or memory body for a new role. Unknown fields must fail deserialization.

Extend the resolved world-knowledge function:

```rust
pub(super) fn resolve_world_knowledge(
    story_ctx: &StoryContext,
    config: &ExtractConfig,
    fact_proposals: Vec<FactProposal>,
    rumor_proposals: Vec<RumorProposal>,
    roles_for_memory: Vec<RoleId>,
    new_role_proposals: Vec<NewRoleProposal>,
) -> Result<WorldKnowledgeExtraction, PipelineError>;
```

Add deterministic allocators:

```rust
pub(super) fn allocate_role_id(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
    reserved: &BTreeSet<RoleId>,
) -> Result<RoleId, PipelineError>;

pub(super) fn allocate_initial_memory_id(
    next_turn: &TurnNumber,
    seq: usize,
    reserved: &BTreeSet<MemoryId>,
) -> Result<MemoryId, PipelineError>;
```

`allocate_initial_memory_id` is scoped to one new role and must not require that role to exist in `StoryContext`.

### 3.4 Stage Transport Contracts

Update the stage output and commit input:

```rust
#[derive(Serialize)]
pub struct ExtractOutput {
    pub knowledge_change: KnowledgeChange,
    pub role_additions: Vec<RoleAddOp>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

#[derive(Serialize)]
pub struct CommitInput {
    pub player_contribution: PlayerContribution,
    pub idempotency_key: IdempotencyKey,
    pub turn_segment: TurnSegment,
    pub knowledge_change: KnowledgeChange,
    pub role_additions: Vec<RoleAddOp>,
    pub turn_status: TurnStatus,
    pub turn_evaluation: TurnEvaluation,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}
```

Runtime must move `extract_output.role_additions` directly into `CommitInput.role_additions`. Commit must move that field directly into `StoryCommit.role_additions`.

### 3.5 Prompt Variables and Protocol

Keep the existing prompt IDs:

```text
extract.extract_world_knowledge
extract.extract_role_memory
```

Add world-prompt variables:

```rust
const MAX_ROLE_ADDITIONS_VAR: &str = "max_role_additions";
const MAX_ROLES_FOR_MEMORY_VAR: &str = "max_roles_for_memory";
const MAX_MEMORY_EXTRACTIONS_VAR: &str = "max_memory_extractions";
```

The world prompt receives:

```text
max_role_additions = min(
    config.max_role_additions_per_turn,
    MAX_ROLES_PER_STORY.saturating_sub(story_ctx.roles.len())
)
max_roles_for_memory = config.max_roles_for_memory
max_memory_extractions = config.max_memory_extractions_per_turn
```

The role-memory prompt input must support both target forms:

```rust
pub(super) enum ExtractRoleMemoryTarget<'a> {
    Existing {
        role: &'a RoleEntry,
    },
    New {
        role_id: &'a RoleId,
        spec: &'a RoleSpec,
        profile: &'a CharacterProfile,
    },
}

pub(super) struct ExtractRoleMemoryPrompt<'a> {
    pub story_pack: &'a StoryPack,
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub target: ExtractRoleMemoryTarget<'a>,
    pub config: &'a ExtractConfig,
}
```

For `Existing`, `existing_memories` is the bounded projection of the role's runtime memories and Add/Update are allowed. For `New`, `existing_memories` is `[]`, the target spec/profile is supplied, and only Add is allowed.

The role-memory LLM response shape remains:

```json
{
  "memories": [
    {
      "op": "add",
      "body": {
        "content": "bounded text",
        "activation": {},
        "retrieval": "retrieval hint"
      }
    }
  ]
}
```

All CSI, RC, and FTI templates for both prompt IDs must exist and remain registered in `assets/prompts/index.toml`.

### 3.6 Memory Work and Assembly Contracts

Represent extraction work without exposing it outside the stage:

```rust
pub(super) enum MemoryExtractionTask<'a> {
    Existing {
        role_id: &'a RoleId,
    },
    New {
        role: &'a AllocatedNewRole,
    },
}

pub(super) enum MemoryExtractionResult {
    Existing(Vec<RoleMemoryChangeOp>),
    New {
        role_id: RoleId,
        memories: BTreeMap<MemoryId, MemoryBody>,
    },
}
```

Assembly must return separate payloads:

```rust
pub(super) fn assemble_extract_changes(
    world_extraction: WorldKnowledgeExtraction,
    memory_results: Vec<MemoryExtractionResult>,
) -> Result<(KnowledgeChange, Vec<RoleAddOp>), PipelineError>;
```

The output order of `RoleAddOp` values must match the accepted `new_roles` proposal order.

### 3.7 Persistence Application Contract

`StoryStore::commit` must use helpers with these responsibilities:

```rust
fn validate_role_additions(
    original: &StoryContext,
    role_additions: &[RoleAddOp],
) -> Result<(), PersistenceError>;

fn validate_memory_change_targets(
    original: &StoryContext,
    changes: &[RoleMemoryChangeOp],
) -> Result<(), PersistenceError>;

fn apply_role_additions(
    next: &mut StoryContext,
    turn_number: TurnNumber,
    role_additions: Vec<RoleAddOp>,
) -> Result<(), PersistenceError>;
```

`apply_role_additions` creates:

```rust
RoleEntry {
    id: operation.id,
    origin: EntryOrigin::Turn(turn_number),
    spec: operation.spec,
    profile: operation.profile,
    memories: operation
        .initial_memories
        .into_iter()
        .map(|(id, body)| {
            (
                id.clone(),
                MemoryEntry {
                    id,
                    origin: EntryOrigin::Turn(turn_number),
                    body,
                },
            )
        })
        .collect(),
}
```

The implementation may reorganize helper parameters to satisfy ownership, but it must preserve the exact validation order and state transition in §4.

### 3.8 File Layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/
│   │   ├── extract-extract-world-knowledge.md.j2
│   │   └── extract-extract-role-memory.md.j2
│   ├── rc/
│   │   ├── extract-extract-world-knowledge.md.j2
│   │   └── extract-extract-role-memory.md.j2
│   └── fti/
│       ├── extract-extract-world-knowledge.md.j2
│       └── extract-extract-role-memory.md.j2
└── src/
    ├── core/
    │   ├── role/role_change.rs
    │   └── story/story_commit.rs
    ├── persistence/story_store_mem.rs
    └── pipeline/
        ├── extract/
        │   ├── config.rs
        │   ├── extract.rs
        │   ├── extract_knowledge.rs
        │   ├── extract_llm.rs
        │   ├── extract_prompt.rs
        │   └── tests/
        ├── commit/commit.rs
        └── runtime/runtime.rs
```

---

## 4. Behavior Rules

1. **P1-R1 Proposal boundary**: World extraction may emit only `RoleSpec` and `CharacterProfile` for each new role. Any model-supplied role ID, memory ID, or private memory field is an unknown-field parse failure.
2. **P1-R2 Effective addition budget**: Before rendering the world prompt, compute `effective_max_role_additions = min(config.max_role_additions_per_turn, MAX_ROLES_PER_STORY.saturating_sub(story_ctx.roles.len()))`.
3. **P1-R3 Output budget validation**: Rust rejects output when `new_roles.len() > effective_max_role_additions`, `roles_for_memory.len() > max_roles_for_memory`, or checked addition of both lengths exceeds `max_memory_extractions_per_turn`.
4. **P1-R4 Existing-role selection**: Every `roles_for_memory` ID must exist in the original snapshot, must not equal another selected ID, and may not refer to any same-Turn new role.
5. **P1-R5 Role validation**: Validate each proposed `RoleSpec`, alias count, and profile before role-ID allocation or memory extraction.
6. **P1-R6 Role-ID allocation**: For proposal order `seq >= 1`, test `role_t{turn}_{candidate_seq}` values until one is absent from both `story_ctx.roles` and the batch reserved set. Insert the chosen ID into the reserved set before processing the next proposal.
7. **P1-R7 Determinism**: The same `StoryContext`, Turn number, configuration, and parsed LLM response produce the same role IDs and preserve proposal order.
8. **P1-R8 Shared concurrency**: Existing-role and new-role memory tasks form one ordered work list and execute through one `buffered(config.max_memory_concurrency.get())` or equivalent bounded ordered stream. Do not create separate fan-out pools.
9. **P1-R9 New-role isolation**: A new-role memory prompt receives that role's assigned ID, spec, profile, the finalized proposal text, and an empty memory list. It receives no other role's private memories.
10. **P1-R10 World privacy**: World-knowledge prompt input and output contain the role catalog but no private role memory content.
11. **P1-R11 Initial-memory operations**: A new-role memory response may contain only Add proposals. Update or Remove returns `PipelineError::InvalidOutput` with a stable Extract code.
12. **P1-R12 Memory-ID allocation**: For each new role independently, assign accepted memory proposals `mem_t{turn}_{candidate_seq}` with `seq >= 1`, skipping IDs in that role's reserved set. The allocator does not query `story_ctx.roles`.
13. **P1-R13 Memory limits**: Each initial-memory map must contain at most both `config.max_memory_changes_per_role` and `MAX_MEMORIES_PER_ROLE` entries. Validate every `MemoryBody` activation rule before assembly.
14. **P1-R14 Separate transport**: `ExtractOutput.role_additions`, `CommitInput.role_additions`, and `StoryCommit.role_additions` remain separate from `KnowledgeChange` and `Turn` at every boundary.
15. **P1-R15 Original-snapshot reference checks**: Before cloning or mutating context, commit validates that every role-addition ID is absent from the original roles, addition IDs are unique, and every `RoleMemoryChangeOp.role_id` exists in the original roles.
16. **P1-R16 Same-Turn memory isolation**: A `RoleMemoryChangeOp` targeting a role in `role_additions` is rejected even if that ID would exist after applying additions. New-role memories may arrive only through the matching `RoleAddOp.initial_memories`.
17. **P1-R17 Candidate application order**: After all original-snapshot checks pass, clone the context, apply all role additions, apply knowledge changes, then validate all candidate roles using Phase 0 shared validators.
18. **P1-R18 Atomic rejection**: Duplicate IDs, invalid references, invalid bodies, per-role limits, cumulative role count, catalog bytes, or total role-state bytes reject the entire commit without changing visible stored context, Turn history, idempotency records, or pack state.
19. **P1-R19 Origins**: A committed dynamic role and every one of its initial memories use `EntryOrigin::Turn(committed_turn_number)`.
20. **P1-R20 Replay**: Replaying an already committed idempotency key returns the stored `StoryCommit`, including identical role additions, and does not insert the roles or memories again.
21. **P1-R21 Next-Turn equivalence**: After successful commit, dynamic roles are represented only as ordinary `RoleEntry` values and participate in the next Turn's role catalog, planning, thinking, generation, validation, repair, and memory extraction exactly like seed roles.
22. **P1-R22 Prompt registration**: Both changed prompt IDs retain exactly three registered CSI/RC/FTI templates. Rust contains no hardcoded replacement prompt text.

### 4.1 Error Handling

- Invalid LLM JSON returns `PipelineError::InvalidOutput` with stage `Extract`, a stable code, and JSON line/column.
- Budget, unknown-role, duplicate-role, forbidden operation, and generated-ID failures return distinct stable Extract error codes.
- Commit validation failures return typed `PersistenceError::ConstraintViolation` or a more specific existing/new typed variant; no partial mutation is permitted.
- ID construction errors map to diagnosable typed errors; no external or LLM input path may use `unwrap` or `expect`.
- `aise-core` must not expose `anyhow::Error`.

### 4.2 Concurrency

- Every LLM call routes through the injected `LlmGateway`.
- At most `max_memory_concurrency` role-memory LLM calls are in flight across existing and new roles combined.
- Preserve task input order in collected results so role-addition ordering is deterministic.
- No lock guard crosses `.await`; extraction operates only on the read-only Turn snapshot.
- Do not introduce a queue, detached task, unbounded stream, or background worker.

### 4.3 Observability

- Reuse Extract stage and subtask observations with explicitly passed parents.
- Create one role-memory sub-observation per existing or new target using the assigned `RoleId`.
- World extraction observation may record bounded role proposals but must not contain private memories.
- Role-memory prompt observation may record only the target role and the bounded memory content actually sent to the LLM.
- Extract and commit observations may record current-Turn `RoleAddOp.initial_memories`; they must not attach full unchanged memory maps from other roles.
- Tests must use unique sentinel text to prove unselected and unchanged private memories are absent from stage, prompt, and commit observation content.
- Add no speculative metric or trace metadata.

---

## 5. Acceptance Criteria

- [ ] `RoleAddOp` exists at `core/role/role_change.rs` with exactly `id`, `spec`, `profile`, and `initial_memories`.
- [ ] `StoryCommit` has `role_additions`; `Turn` and `KnowledgeChange` do not.
- [ ] `ExtractConfig` defaults are `max_role_additions_per_turn = 2` and `max_memory_extractions_per_turn = 4`.
- [ ] Configuration tests cover every invalid inequality in §3.2 at the boundary and one over.
- [ ] World response parsing rejects role IDs, memory IDs, memory bodies, unknown fields, malformed JSON, and over-budget arrays.
- [ ] Prompt tests prove the effective role-addition budget shrinks at `MAX_ROLES_PER_STORY - 1` and becomes zero at the limit.
- [ ] Prompt tests prove world extraction contains no private-memory sentinel.
- [ ] Existing and new memory tasks share a measured peak concurrency no greater than `max_memory_concurrency`.
- [ ] New-role memory prompt tests prove empty prior memories, assigned role ID, matching spec/profile, and no other-role sentinel.
- [ ] New-role memory parsing rejects Update and Remove operations.
- [ ] Role allocator tests cover collision with a seed role, collision with an existing dynamic role, collision inside one batch, and deterministic replay.
- [ ] Initial-memory allocator tests cover role-local collision skipping and deterministic IDs.
- [ ] `ExtractOutput`, `CommitInput`, and `StoryCommit` preserve role-addition order and payload without cloning unrelated role state.
- [ ] Commit rejects duplicate addition IDs and an addition colliding with an original role.
- [ ] Commit rejects every memory op targeting a same-Turn new role.
- [ ] Commit accepts initial memories only through the matching `RoleAddOp`.
- [ ] Commit boundary tests cover exact and one-over cumulative role count, catalog bytes, per-role memories, and total role-state bytes after applying all changes.
- [ ] Every failed commit leaves context, recent turns, idempotency records, and role maps byte-equivalent to pre-commit state.
- [ ] Successful commit assigns matching `EntryOrigin::Turn` to the role and all initial memories.
- [ ] Idempotent replay returns identical role additions and performs no duplicate insertion.
- [ ] The next-Turn role catalog includes the committed dynamic role using the same projection as seed roles.
- [ ] Extract/commit observation tests prove unchanged and unselected memory sentinels are absent.
- [ ] `assets/prompts/index.toml` references existing CSI, RC, and FTI files for both Extract prompt IDs.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test -p aise-core` passes.
- [ ] `cargo clippy -p aise-core --all-targets -- -D warnings` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Existing-role spec/profile updates, role deletion, and player-role reassignment require a separate design and spec.
- Cross-Turn role archival or selective role eviction is not introduced; story deletion remains the cleanup boundary.
- Production SQLite schema migration remains deferred until SQLite stops delegating to the in-memory backend.

---

## 7. References

- Source design: [Seed 与 Runtime 角色及故事上下文解耦](../../design/2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)
- Phase 0 prerequisite: [Role and Context Runtime Decoupling Phase 0](./2026-10-17-role-context-runtime-decoupling-spec-phase-0-gpt.md)
- Architecture baseline: [Pipeline 框架与编排](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Architecture and refactor guardrails: [architecture-refactor.md](../../agents/guardrails/architecture-refactor.md)
- Layer guardrails: [layer-dependencies.md](../../agents/guardrails/layer-dependencies.md)
- Pipeline guardrails: [pipeline.md](../../agents/guardrails/pipeline.md)
- Code organization guardrails: [code-organization.md](../../agents/guardrails/code-organization.md)
- Observability guardrails: [observability.md](../../agents/guardrails/observability.md)
