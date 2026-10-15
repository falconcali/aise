# Character-Aware Generation and Verification — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-15
> **Status**: Proposed
> **Source Design**: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
> **Phase**: Phase 3

---

## 1. Goal

Supply bounded effective role data and ordered Character Decisions to Story Generation, verify the complete Turn flow, and remove every superseded placeholder path.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add Generate-owned role-context budgets.
- Extend `GenerateInput` with resolved roles and Character Decisions.
- Render role data and decisions into the Generate RC prompt.
- Preserve character knowledge boundaries and player agency in Generate instructions.
- Complete Runtime data flow from Plan through Retrieval and Think into Generate.
- Prove Character Decisions never enter `StoryCommit`.
- Add maintained-crate end-to-end tests for empty, single-role, Card, multi-role, and failure flows.
- Remove the Think echo placeholder and every obsolete prompt or contract.
- Run formatting, clippy with warnings denied, and maintained-crate tests.

### 2.2 Non-Goals

- Does not persist Character Decisions or expose them through external APIs.
- Does not add runtime role-memory mutation, eviction, or storage.
- Does not add structured presence, location, emotion, or role state.
- Does not add an independent role-selection LLM call.
- Does not re-plan after Character Think.
- Does not let Character Decisions commit world facts.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0–2 are required.
- All four phases MUST be reviewed and landed as one hard-refactor change.
- Runtime remains the only module that imports Plan, Retrieval, Think, and Generate stage contracts.
- Generate MUST NOT import Plan, Retrieval, Think, or Runtime.
- Prompt text lives only in Prompt assets.
- Context projection is deterministic, bounded, and pure after its inputs are available.
- No compatibility parser, hidden fallback, partial Character Decision path, or deprecated placeholder is allowed.
- Tests follow `tests/<source>_tests.rs`; no inline test modules.
- No code comments, `#[allow]`, ignored tests, or undocumented waivers are permitted.

---

## 3. Contracts

### 3.1 Generate configuration

Create `crates/aise-core/src/pipeline/generate/config.rs`:

```rust
use serde::Deserialize;

const DEFAULT_MAX_ROLE_BYTES_PER_ROLE: usize = 16 * 1024;
const DEFAULT_MAX_ROLE_CONTEXT_BYTES: usize = 384 * 1024;
const DEFAULT_MAX_SEED_MEMORIES_PER_ROLE: usize = 4;
const DEFAULT_MAX_DECISION_CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateConfig {
    pub max_role_bytes_per_role: usize,
    pub max_role_context_bytes: usize,
    pub max_seed_memories_per_role: usize,
    pub max_decision_context_bytes: usize,
}
```

`GenerateConfig::default()` returns `16384`, `393216`, `4`, and `16384`.

Extend:

```rust
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub generate: GenerateConfig,
    pub summary: SummaryConfig,
}
```

Re-export `GenerateConfig` through `pipeline/generate/mod.rs` and `pipeline/mod.rs`.

### 3.2 Generate stage contract

The final input is:

```rust
#[derive(Serialize)]
pub struct GenerateInput<'a> {
    pub story_goal: &'a str,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub roles: &'a [ResolvedRole],
    pub character_decisions: &'a [CharacterDecision],
}
```

`GeneratePipeline` MUST own:

```rust
pub struct GeneratePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: GenerateConfig,
}

impl GeneratePipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: GenerateConfig,
    ) -> Self;
}
```

### 3.3 Generate role projection

Create `pipeline/generate/generate_roles.rs`:

```rust
pub(super) struct GenerateRoleContext {
    pub roles: String,
    pub decisions: String,
}

pub(super) fn project_role_context(
    roles: &[ResolvedRole],
    decisions: &[CharacterDecision],
    config: &GenerateConfig,
) -> Result<GenerateRoleContext, PipelineError>;
```

For each role, render these fields in order:

```text
role_id
name
aliases
profile.appearance
profile.personality
profile.speaking_style
profile.dialogue_examples
backstory
seed_memories
```

Rules:

1. roles preserve `RetrievalOutput.roles` order;
2. every role renders RoleId, name, aliases, and the complete effective profile as compact JSON;
3. Card metadata and Card name are never rendered;
4. backstory is rendered after profile;
5. seed memories preserve `MemoryId` order and stop after `max_seed_memories_per_role`;
6. each rendered role is capped at `max_role_bytes_per_role`;
7. if the full backstory does not fit the remaining per-role budget, render the longest UTF-8-safe prefix followed by the literal marker `[truncated]`;
8. seed memories are added as complete items only; the first item that does not fit ends memory projection for that role;
9. aggregate role output is capped at `max_role_context_bytes`;
10. if mandatory identity and profile fields cannot fit, return `generate_role_projection_too_large`;
11. if all individually bounded roles exceed the aggregate budget, return `generate_role_context_too_large`;
12. no role is silently omitted.

Character Decisions render in input order:

```text
- role_id: "<JSON-escaped RoleId>"
  decision: "<JSON-escaped decision>"
  suggested_utterance: "<JSON-escaped string or null>"
```

The complete decision rendering MUST fit `max_decision_context_bytes`; otherwise return `generate_decision_context_too_large`. Decisions are never truncated.

Before rendering, validate:

- every decision RoleId is unique;
- every decision RoleId exists in `roles`;
- no decision targets `StoryContext.player`, enforced by the caller before projection.

Use:

```text
generate_duplicate_character_decision
generate_character_decision_role_missing
generate_player_character_decision_forbidden
generate_role_projection_too_large
generate_role_context_too_large
generate_decision_context_too_large
```

All map to `PipelineError::InconsistentData` with `stage = PipelineStage::Generate`.

### 3.4 Generate prompt protocol

Add Prompt variables:

```text
relevant_roles
character_decisions
```

The Generate RC section order is:

```text
Story Summary or Story Opening
Recent Story
Known Facts
Rumors
Relevant Roles
Character Decisions
Narrative Direction
Story Constraints
Immediate Story Goal
Pending Player Contribution
```

Generate CSI/FTI MUST state:

- Relevant Roles are author-side consistency data.
- A role's private backstory or seed memory is not automatically known by another role.
- A Character Decision is the named role's current viewpoint and immediate intent, not a world fact.
- `suggested_utterance` is optional guidance, not mandatory verbatim dialogue.
- The segment must preserve a Character Decision unless a visible causal event in the segment changes it.
- Character Decisions do not override established Facts, player contribution, or Story Constraints.
- The player role may appear in Relevant Roles but never in Character Decisions.
- The generator must not make an unprovided consequential choice for the player.
- The generator returns story prose only.

### 3.5 Runtime final data flow

After Retrieval:

```rust
let think_output = if plan_output.requires_character_thinking() {
    let think_input = ThinkInput {
        requests: &plan_output.character_think_requests,
        resolved_roles: &retrieval_output.roles,
        player_contribution: &baseline_output.player_contribution,
        directions: &baseline_output.narrative.directions,
    };
    pipeline_runner.run(&self.think, think_input, story_ctx).await?
} else {
    ThinkOutput::default()
};
```

`ThinkOutput` MUST derive or implement `Default` as an empty decision vector.

Generate receives:

```rust
let generate_input = GenerateInput {
    story_goal: &plan_output.story_goal,
    player_contribution: &baseline_output.player_contribution,
    directions: &baseline_output.narrative.directions,
    constraints: &baseline_output.narrative.constraints,
    knowledge: &baseline_output.knowledge,
    retrieved_facts: &retrieval_output.facts,
    retrieved_rumors: &retrieval_output.rumors,
    roles: &retrieval_output.roles,
    character_decisions: &think_output.decisions,
};
```

`CharacterDecision` MUST NOT appear in `CommitInput`, `StoryCommit`, `StoryContext`, `StoryStore`, or Pack persistence.

### 3.6 Generate trace

Extend Generate observation metadata with:

```text
aise.generate.roles
aise.generate.character_decisions
aise.generate.role_context_bytes
aise.generate.decision_context_bytes
```

Metrics contain counts and sizes only. Full role or decision content is captured only through the configured `Observation::capture_content` policy.

### 3.7 Required end-to-end cases

Add or extend tests with these exact cases:

```text
pipeline/runtime/tests/runtime_tests.rs
  runtime_skips_role_retrieval_and_think_when_plan_selects_no_roles
  runtime_retrieves_generation_only_role_without_think
  runtime_runs_one_character_think_and_delivers_decision_to_generate
  runtime_stable_union_resolves_cross_list_role_once
  runtime_preserves_multi_role_think_request_order
  runtime_fails_before_generate_when_one_character_think_fails
  runtime_never_places_character_decisions_in_commit

pipeline/generate/tests/generate_roles_tests.rs
  role_projection_preserves_effective_role_order
  role_projection_uses_effective_card_profile_and_role_body_identity
  role_projection_keeps_private_memory_scoped_to_its_role
  role_projection_bounds_seed_memories
  role_projection_rejects_missing_decision_role
  role_projection_rejects_duplicate_decision_role
  role_projection_rejects_player_decision
  role_projection_rejects_aggregate_overflow

pipeline/generate/tests/generate_prompt_tests.rs
  generate_prompt_contains_roles_and_ordered_decisions
  generate_prompt_marks_decisions_as_viewpoint_not_fact
  generate_prompt_preserves_player_agency
```

### 3.8 Forbidden residuals

These patterns MUST have zero matches in maintained implementation code:

```text
ThinkInput { query:
ThinkOutput { result:
result: input.query.into()
requires_character_thinking(&self) -> bool { false }
```

No prompt or parser may accept:

```text
character_id
role_name as identity
plain-text Character Think output
Markdown-fenced Character Think JSON
```

### 3.9 Verification commands

Run from the repository root:

```powershell
cargo fmt -p aise-core -p aise-service -- --check
cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings
cargo test -p aise-core -p aise-service
rg "ThinkInput\s*\{\s*query|ThinkOutput\s*\{\s*result|result:\s*input\.query" crates/aise-core
rg "requires_character_thinking\(&self\).*false" crates/aise-core
rg "LlmProvider" crates/aise-core/src/pipeline
```

The three `rg` commands MUST exit with no forbidden matches. Frozen crates are excluded from all edits.

### 3.10 File layout

```text
crates/aise-core/src/pipeline/
  common/
    config.rs
  generate/
    mod.rs
    config.rs
    generate.rs
    generate_prompt.rs
    generate_roles.rs
    generate_trace.rs
    tests/
      config_tests.rs
      generate_prompt_tests.rs
      generate_roles_tests.rs
  runtime/
    runtime.rs
    tests/
      runtime_tests.rs
```

---

## 4. Behavior Rules

1. **R-CT-P3-1**: Generate receives every effective role exactly once in stable union order.
2. **R-CT-P3-2**: Generate receives one decision for every successful Think request in request order.
3. **R-CT-P3-3**: A generation-only role may have no Character Decision.
4. **R-CT-P3-4**: A Think target is available to Generate even when absent from `roles_for_generation`.
5. **R-CT-P3-5**: Player-role data may be rendered as Relevant Role data, but no player Character Decision is accepted.
6. **R-CT-P3-6**: Private role data informs author consistency but never grants knowledge to another role.
7. **R-CT-P3-7**: Character Decisions are viewpoint guidance and never establish world truth.
8. **R-CT-P3-8**: Any Retrieval or Think failure prevents Generate execution.
9. **R-CT-P3-9**: Generate projection overflow fails diagnostically; it never silently drops a role or decision.
10. **R-CT-P3-10**: Character Decisions are released with the `run_turn` stack frame and never enter Commit.
11. **R-CT-P3-11**: Runtime stage topology remains Baseline → Plan → optional Retrieval → optional Think → Generate → Validate/Repair → Extract → Summary → Commit.
12. **R-CT-P3-12**: No stage calls another stage or imports another stage's contract.

### 4.1 Error Handling

- Generate projection uses the exact stable codes in §3.3.
- Retrieval and Think errors propagate unchanged through Runtime.
- No stage catches an error to continue with empty roles or partial decisions.
- No `unwrap`, `expect`, or panic is allowed for role or decision projection.

### 4.2 Concurrency

- Generate performs one LLM call through the shared Gateway limiter.
- Role and decision projection is synchronous, deterministic, and bounded.
- Runtime awaits the complete Think stage before constructing `GenerateInput`.
- No role, decision, prompt buffer, task, or permit outlives the Turn.

### 4.3 Observability

- Plan, Retrieval, Think, and Generate observations expose counts and stable error codes sufficient to locate a failed handoff.
- Identifiers are structured fields; messages do not interpolate them.
- Logs and metadata exclude full profile, backstory, memory, decision, utterance, and generated prose.
- Stage input/output capture remains subject to the configured content-capture policy.

---

## 5. Acceptance Criteria

- [ ] `GenerateConfig` exists, is aggregated, injected, and defaults match §3.1.
- [ ] `GenerateInput` and `GeneratePipeline` match §3.2.
- [ ] Role projection includes every effective role and never substitutes Card identity for RoleBody identity.
- [ ] Role and decision projection enforce all configured byte and count limits.
- [ ] Generate prompt assets contain both new sections in §3.4 order.
- [ ] Prompt tests prove private memory is not described as shared character knowledge.
- [ ] Prompt tests prove decisions are not world facts and utterances are not mandatory scripts.
- [ ] Runtime implements the exact data flow in §3.5.
- [ ] All end-to-end cases in §3.7 pass.
- [ ] `CharacterDecision` has zero references in Commit, StoryCommit, StoryContext, and persistence modules.
- [ ] All forbidden residual searches in §3.8 return zero matches.
- [ ] `cargo fmt`, clippy with `-D warnings`, and all maintained-crate tests pass.
- [ ] `git diff -- crates/aise crates/aise-server` is empty.
- [ ] After every criterion passes, update the source design status from `Draft` to `Accepted`.

---

## 6. Out of Scope / Future Work

- Runtime role-memory creation, update, retrieval ranking, eviction, and persistence require a separate design.
- Structured presence, location, emotion, and role-state extraction require separate evidence before design.
- Splitting role selection into a second LLM call requires measured Plan-quality degradation and a new design.

---

## 7. References

- Source design: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
- Phase 0: [Role Catalog and Contracts](./2026-10-15-character-think-pipeline-spec-phase-0-gpt.md)
- Phase 1: [Role Retrieval and Runtime Handoff](./2026-10-15-character-think-pipeline-spec-phase-1-gpt.md)
- Phase 2: [Character Think Execution](./2026-10-15-character-think-pipeline-spec-phase-2-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
