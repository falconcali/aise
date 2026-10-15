# Character Role Retrieval and Runtime Handoff — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-15
> **Status**: Proposed
> **Source Design**: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
> **Phase**: Phase 1

---

## 1. Goal

Resolve the stable union of generation and Character Think RoleIds into complete effective role data through one Retrieval stage.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Compute a stable unique effective role set in `Runtime::run_turn`.
- Extend Retrieval input and output with RoleIds and `ResolvedRole`.
- Resolve role identity and private data from the current Story Pack.
- Resolve effective profiles from `RoleActor::Default` or `RoleActor::Card`.
- Preserve RoleBody identity when a Character Card supplies the profile.
- Return diagnosable errors for missing roles, cast bindings, cards, and duplicate requests.
- Record requested, resolved, Default, and Card counts.
- Pass resolved roles to later phases through Runtime-owned Turn data.

### 2.2 Non-Goals

- Does not call an LLM in Retrieval.
- Does not rank, truncate, activate, or partially resolve requested roles.
- Does not select memories for a prompt.
- Does not execute Character Think or modify Generate.
- Does not persist, cache, or mutate resolved role data.
- Does not introduce runtime character memory.

### 2.3 Implementation Constraints

- Phase 0 is required.
- Phases 0–3 MUST land as one hard-refactor change.
- Runtime is the only module that imports Plan and Retrieval stage contracts.
- Retrieval MUST NOT import Plan, Think, Generate, or Runtime.
- Retrieval accesses assets only through the injected `Store` and `PackStore`.
- The current `StoryContext.pack_ref` and `StoryContext.cast` are authoritative.
- No skipped RoleId, placeholder profile, fallback Card, partial output, or compatibility path is allowed.
- No code comments are permitted.

---

## 3. Contracts

### 3.1 Stable effective role set

Add a pure helper to `pipeline/runtime/runtime.rs`:

```rust
fn effective_role_ids(plan_output: &PlanOutput) -> Vec<RoleId>;
```

The helper:

1. visits `roles_for_generation` in output order;
2. visits `character_think_requests` in output order;
3. appends each RoleId only on its first occurrence.

It MUST use a membership set only for deduplication; the returned vector defines the authoritative retrieval and Generate role order.

### 3.2 Retrieval stage contract

Extend the existing contracts:

```rust
#[derive(Serialize)]
pub struct RetrievalInput<'a> {
    pub fact_ids: &'a [FactId],
    pub rumor_ids: &'a [RumorId],
    pub role_ids: &'a [RoleId],
}

#[derive(Default, Serialize)]
pub struct RetrievalOutput {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
    pub roles: Vec<ResolvedRole>,
}

impl RetrievalOutput {
    pub fn role(&self, role_id: &RoleId) -> Option<&ResolvedRole>;
}
```

`RetrievalOutput::role` returns the single matching role by stable `RoleId`. It MUST NOT compare name or alias.

### 3.3 Role resolution

Extend `RetrievalPipeline` with:

```rust
async fn resolve_roles(
    &self,
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    role_ids: &[RoleId],
) -> Result<Vec<ResolvedRole>, PipelineError>;

async fn resolve_role(
    &self,
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    role_id: &RoleId,
) -> Result<ResolvedRole, PipelineError>;

async fn resolve_profile(
    &self,
    role_id: &RoleId,
    role_body: &RoleBody,
    actor: &RoleActor,
) -> Result<CharacterProfile, PipelineError>;
```

The resulting value is:

```rust
ResolvedRole {
    role_id: role_id.clone(),
    name: role_body.name.clone(),
    aliases: role_body.alias.clone(),
    backstory: role_body.backstory.clone(),
    retrieval_hint: role_body.retrieval.clone(),
    profile,
    seed_memories: role_body.seed_memories.clone(),
}
```

Profile resolution is exact:

```text
RoleActor::Default
  -> clone RoleBody.default_profile

RoleActor::Card(card_ref)
  -> PackStore::get_character_card(card_ref)
  -> clone CharacterCard.profile
```

Card `name`, `card_id`, metadata, and tags MUST NOT replace any `RoleBody` field.

### 3.4 Retrieval consistency errors

Use these stable codes:

```text
retrieval_duplicate_role_id
retrieval_role_missing
retrieval_role_cast_missing
retrieval_role_card_missing
```

The first three map to:

```rust
PipelineError::InconsistentData {
    stage: PipelineStage::Retrieval,
    code,
    message,
}
```

For a Card actor:

- `PersistenceError::NotFound` maps to `retrieval_role_card_missing`.
- Every other `PersistenceError` remains `PipelineError::Persistence` with `stage = Retrieval`.

### 3.5 Retrieval execution

The Retrieval primary flow is:

```text
load StoryPack once
resolve requested Facts in request order
resolve requested Rumors in request order
validate RoleId uniqueness
resolve requested Roles in request order
return one RetrievalOutput
```

If any requested item fails, Retrieval returns `Err` and discards the entire local output. No downstream stage receives partial data.

### 3.6 Runtime handoff

`Runtime::run_turn` MUST construct:

```rust
let role_ids = effective_role_ids(&plan_output);

let retrieval_input = RetrievalInput {
    fact_ids: &plan_output.facts_to_retrieve,
    rumor_ids: &plan_output.rumors_to_retrieve,
    role_ids: &role_ids,
};
```

Retrieval executes once when `plan_output.requires_retrieval()` is true. When it is false, Runtime uses `RetrievalOutput::default()`.

The Turn-scoped flow is:

```text
PlanOutput
  -> Runtime stable union
  -> one RetrievalInput
  -> one RetrievalOutput
  -> borrowed resolved roles for Think and Generate
```

`role_ids`, `RetrievalOutput`, and every `ResolvedRole` remain local to the `run_turn` stack frame and are dropped after the Turn.

### 3.7 Retrieval trace

Extend `RetrievalTraceMetrics`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalTraceMetrics {
    pub facts_requested: usize,
    pub facts_resolved: usize,
    pub rumors_requested: usize,
    pub rumors_resolved: usize,
    pub roles_requested: usize,
    pub roles_resolved: usize,
    pub default_roles_resolved: usize,
    pub card_roles_resolved: usize,
}
```

Add metadata:

```text
aise.retrieval.roles_requested
aise.retrieval.roles_resolved
aise.retrieval.roles_default
aise.retrieval.roles_card
```

The observation remains:

```text
name = "retrieve_knowledge"
kind = ObservationKind::Retriever
```

### 3.8 File layout

```text
crates/aise-core/src/pipeline/
  retrieval/
    mod.rs
    retrieval.rs
    retrieval_trace.rs
    tests/
      retrieval_tests.rs
      retrieval_trace_tests.rs
  runtime/
    runtime.rs
    tests/
      runtime_tests.rs
```

---

## 4. Behavior Rules

1. **R-CT-P1-1**: Effective roles preserve first occurrence across generation roles followed by Think targets.
2. **R-CT-P1-2**: A RoleId present in both Plan fields is resolved exactly once.
3. **R-CT-P1-3**: Retrieval output role order equals effective input role order.
4. **R-CT-P1-4**: Every resolved identity field, backstory, retrieval hint, and seed memory comes from `RoleBody`.
5. **R-CT-P1-5**: A Character Card replaces only the effective profile.
6. **R-CT-P1-6**: Retrieval does not use `RoleBody.activation`.
7. **R-CT-P1-7**: Retrieval does not inspect story prose to infer role presence or relevance.
8. **R-CT-P1-8**: Duplicate RoleIds in direct Retrieval input fail before any Card lookup.
9. **R-CT-P1-9**: Missing role, cast, or Card data fails the whole Retrieval stage.
10. **R-CT-P1-10**: Retrieval output is read-only Turn data and is never committed.

### 4.1 Error Handling

- Errors use the exact stable codes from §3.4.
- Returned diagnostics may identify the relevant RoleId; logs MUST use structured `role_id` fields and MUST NOT interpolate IDs into log messages.
- No missing role or Card may fall back to another role, `default_profile`, or an empty profile.
- No `unwrap`, `expect`, or panic is allowed on Pack, cast, Card, or RoleId lookup.

### 4.2 Concurrency

- Fact, Rumor, and Default-role resolution are synchronous after the Story Pack is loaded.
- Card lookups execute sequentially in effective role order in this phase.
- No lock or guard may cross an `.await`.
- Retrieval creates no background task, channel, cache, or unbounded future set.

### 4.3 Observability

- `finish_retrieval` reports all eight counters in §3.7 on success and failure.
- Logs and metadata contain counts, role kind, stable error code, and structured RoleId only.
- Full profile, backstory, seed-memory content, and Card content MUST NOT be placed in metadata or log messages.
- Observation input/output content continues to pass through `Observation::capture_content`.

---

## 5. Acceptance Criteria

- [ ] `effective_role_ids` preserves generation order, appends Think-only targets, and removes cross-list duplicates.
- [ ] `RetrievalInput` and `RetrievalOutput` match §3.2.
- [ ] Default-role tests prove `RoleBody.default_profile` is used.
- [ ] Card-role tests prove only `CharacterCard.profile` replaces the default profile.
- [ ] Card-role tests prove Card name does not replace `RoleBody.name`.
- [ ] Role output preserves request order and resolves each unique ID once.
- [ ] Direct duplicate input returns `retrieval_duplicate_role_id`.
- [ ] Missing Pack role, cast binding, and Card return their exact §3.4 codes.
- [ ] Non-NotFound Card store failures remain `PipelineError::Persistence`.
- [ ] Runtime executes one Retrieval stage for knowledge, roles, or both.
- [ ] Runtime skips Retrieval only when all Fact, Rumor, generation-role, and Think-request collections are empty.
- [ ] Retrieval trace tests cover Default-only, Card-only, mixed, and failure cases.
- [ ] `rg "crate::pipeline::(plan|think|generate|runtime)" crates/aise-core/src/pipeline/retrieval -g "*.rs"` returns zero matches.

---

## 6. Out of Scope / Future Work

- Character Think prompt projection and bounded LLM execution are specified in [Phase 2](./2026-10-15-character-think-pipeline-spec-phase-2-gpt.md).
- Generate role projection and Character Decision consumption are specified in [Phase 3](./2026-10-15-character-think-pipeline-spec-phase-3-gpt.md).

---

## 7. References

- Source design: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
