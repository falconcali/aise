# Repair Pipeline and Revalidation Loop — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
> **Phase**: Phase 2

---

## 1. Goal

Implement Repair as a bounded full-story rewrite and make Runtime execute a finite Validate/Repair loop with complete revalidation after every repair.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Replace the placeholder Repair stage with prompt rendering, one `LlmGateway` call, and bounded output validation.
- Give Repair the same generation evidence used by Validate plus only the current proposal's failed issues.
- Add a shared evidence projection used by both Validate and Repair prompts.
- Refactor `Runtime::validate_and_repair` to own and consume the repair budget.
- Re-run all configured checks and scores after every repaired proposal.
- Keep proposal/report history bounded by `max_repair_rounds + 1`.
- Stop before Extract, Summary, and Commit when hard gates remain failed after exhaustion.
- Add Repair and Runtime loop tests.

### 2.2 Non-Goals

- Does not let Repair call Validate or any other stage.
- Does not perform incremental validation of only prior failures.
- Does not expose original or older proposals to Repair.
- Does not rank exhausted score-only candidates; Phase 3 supplies that pure helper.
- Does not persist evaluation data; Phase 3 wires Commit.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0 and 1 are required.
- Phases 0–3 MUST land as one hard-refactor change; Phase 2 MUST call the final Phase 3 ranking contract and MUST NOT add a temporary fallback.
- Runtime remains the only loop orchestrator.
- Runtime consumes budget before invoking Repair.
- Repair and Validate MUST NOT import each other.
- No mutable budget, proposal history, report history, or issue history may be stored on a pipeline instance.
- Rust source remains comment-free and tests use dedicated external test modules.

---

## 3. Contracts

### 3.1 Repair pipeline

Replace `crates/aise-core/src/pipeline/repair/repair.rs` with:

```rust
use crate::core::{StoryContext, TurnControl};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{
    Pipeline, PipelineError, PipelineStage, ValidationContext, ValidationIssue,
};
use crate::pipeline::repair::RepairConfig;
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct RepairInput<'a> {
    pub context: ValidationContext<'a>,
    pub proposal: &'a str,
    pub proposal_version: u32,
    pub issues: &'a [ValidationIssue],
    pub repair_round: u32,
    pub repair_rounds_remaining: u32,
}

#[derive(Serialize)]
pub struct RepairOutput {
    pub proposal: String,
}

pub struct RepairPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: RepairConfig,
}

impl RepairPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: RepairConfig,
    ) -> Self;
}

impl<'a> Pipeline<RepairInput<'a>> for RepairPipeline {
    type Output = RepairOutput;

    fn stage(&self) -> PipelineStage;

    async fn execute(
        &self,
        input: RepairInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError>;
}
```

`execute` follows:

```rust
let repair_observation = repair_trace::begin_repair(observation, &input);
let result = self.repair(&input, story_ctx, control, &repair_observation).await;
repair_trace::finish_repair(repair_observation, &input, &result);
result.map(|proposal| RepairOutput { proposal })
```

### 3.2 Shared prompt evidence

Create `crates/aise-core/src/pipeline/common/validation_prompt.rs`:

```rust
pub(in crate::pipeline) struct ValidationPromptEvidence {
    pub story_summary: String,
    pub story_opening: String,
    pub recent_story: String,
    pub story_goal: String,
    pub player_contribution: String,
    pub narrative_directions: String,
    pub story_constraints: String,
    pub activated_facts: String,
    pub activated_rumors: String,
    pub retrieved_facts: String,
    pub retrieved_rumors: String,
    pub player_role: String,
    pub resolved_roles: String,
    pub character_decisions: String,
}

pub(in crate::pipeline) async fn project_validation_evidence(
    stage: PipelineStage,
    story_ctx: &StoryContext,
    context: &ValidationContext<'_>,
    store: &Store,
) -> Result<ValidationPromptEvidence, PipelineError>;
```

Both `validate_prompt` and `repair_prompt` MUST use this helper. The helper preserves the evidence boundaries from Phase 1 and uses `pipeline_prompt` for shared prior-story, direction, constraint, fact, and rumor rendering. Role and decision projections use deterministic JSON serialization and return `PipelineError::InconsistentData` with the caller's stage if serialization fails.

### 3.3 Repair prompt contract

Register:

```toml
[[prompts]]
id = "repair.repair_story"
csi = "csi/repair-repair-story.md.j2"
rc = "rc/repair-repair-story.md.j2"
fti = "fti/repair-repair-story.md.j2"
```

Create `repair_prompt.rs`:

```rust
pub(super) async fn repair_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &RepairInput<'_>,
    config: &RepairConfig,
    store: &Store,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError>;
```

The rendered variables are exactly:

```text
story_summary
story_opening
recent_story
story_goal
player_contribution
narrative_directions
story_constraints
activated_facts
activated_rumors
retrieved_facts
retrieved_rumors
player_role
resolved_roles
character_decisions
current_proposal
proposal_version
repair_issues
max_proposal_bytes
```

`repair_issues` is a JSON array preserving `ValidationReport::failed_issues()` order:

```json
[
  {
    "kind": "Check",
    "name": "explicit_constraints",
    "message": "The proposal violates the no-violence constraint in the final paragraph."
  }
]
```

CSI defines a story repairer that may change only what is required to resolve listed issues while preserving unaffected content and evidence. RC labels proposal, issues, and all evidence as untrusted content. FTI requires one complete replacement story body, no JSON envelope, no Markdown fence, no analysis, and no issue list.

### 3.4 Repair LLM and output validation

Create `repair_llm.rs`:

```rust
pub(super) async fn repair_story(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError>;
```

It performs exactly one `LlmGateway::complete` call and returns `content`.

Create `repair_output.rs`:

```rust
pub(super) fn validate_repaired_proposal(
    raw: String,
    config: &RepairConfig,
) -> Result<String, PipelineError>;
```

Validation is exact:

```text
raw.trim().is_empty() => PipelineError::EmptyOutput { stage: Repair }
raw.len() > max_proposal_bytes => InvalidOutput code repair_proposal_too_long
otherwise => return raw unchanged
```

Do not strip Markdown fences or extract a substring. Any non-empty in-bound response is the complete next proposal and will be fully validated.

### 3.5 Repair trace

Create `repair_trace.rs`:

```rust
pub(super) fn begin_repair(
    observation: &Observation,
    input: &RepairInput<'_>,
) -> Observation;

pub(super) fn finish_repair(
    observation: Observation,
    input: &RepairInput<'_>,
    result: &Result<String, PipelineError>,
);
```

The observation has:

```text
name = "repair_generation"
kind = ObservationKind::Span
input = capture_content((proposal_version, proposal, issues))
```

Metadata uses:

```text
aise.repair.input_proposal_version
aise.repair.output_proposal_version
aise.repair.round
aise.repair.rounds_remaining
aise.repair.issues
aise.repair.output_bytes
```

`output_proposal_version = input_proposal_version + 1`. Proposal and issue bodies appear only in capture-policy-controlled input/output, never metadata.

### 3.6 Runtime history and evidence

Add private Runtime history:

```rust
struct ValidatedProposal {
    proposal: String,
    report: ValidationReport,
}
```

After Generate, construct every Validate/Repair input from references to the same Turn-scoped values:

```rust
fn validation_context<'a>(
    plan_output: &'a PlanOutput,
    baseline_output: &'a BaselineOutput,
    retrieval_output: &'a RetrievalOutput,
    think_output: &'a ThinkOutput,
) -> ValidationContext<'a>;
```

The mapping is exact:

```text
story_goal         <- plan_output.story_goal
player_contribution <- baseline_output.player_contribution
directions         <- baseline_output.narrative.directions
constraints        <- baseline_output.narrative.constraints
knowledge          <- baseline_output.knowledge
retrieved_facts    <- retrieval_output.facts
retrieved_rumors   <- retrieval_output.rumors
player_role        <- plan_output.player_role
roles              <- retrieval_output.roles
character_decisions <- think_output.decisions
```

Before adding the Generate result to history:

```text
trim empty => PipelineError::EmptyOutput { stage: Generate }
len > repair.max_proposal_bytes => InvalidOutput code generate_proposal_too_long
```

### 3.7 Runtime loop

Replace `validate_and_repair` with a method accepting the full context:

```rust
async fn validate_and_repair(
    &self,
    pipeline_runner: &PipelineRunner<'_>,
    story_ctx: &StoryContext,
    context: ValidationContext<'_>,
    initial_proposal: String,
) -> Result<ValidationLoopResult, PipelineError>;
```

The algorithm is:

```text
history = []
current_proposal = initial_proposal
repairs_used = 0

loop:
  version = history.len() + 1
  report = run Validate(current_proposal, version, repairs_used, max - repairs_used)
  append ValidatedProposal(current_proposal, report) to history

  if latest.report.all_checks_pass && latest.report.all_scores_meet_target:
    return Pass(latest)

  if repairs_used < max_repair_rounds:
    repairs_used += 1
    issues = latest.report.failed_issues()
    current_proposal = run Repair(
      latest.proposal,
      latest.report.proposal_version,
      issues,
      repairs_used,
      max - repairs_used
    )
    continue

  if !latest.report.all_checks_pass:
    return ValidationFailed(
      code = "validation_hard_gate_exhausted",
      proposal_version = latest.report.proposal_version
    )

  return Choice(select_choice(history), repairs_used)
```

The final Choice call uses Phase 3's `select_choice`; no provisional last-version or raw-score fallback is allowed.

### 3.8 File layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/repair-repair-story.md.j2
│   ├── fti/repair-repair-story.md.j2
│   ├── rc/repair-repair-story.md.j2
│   └── index.toml
└── src/pipeline/
    ├── common/
    │   ├── validation_prompt.rs
    │   └── tests/validation_prompt_tests.rs
    ├── repair/
    │   ├── config.rs
    │   ├── mod.rs
    │   ├── repair.rs
    │   ├── repair_llm.rs
    │   ├── repair_output.rs
    │   ├── repair_prompt.rs
    │   ├── repair_trace.rs
    │   └── tests/
    │       ├── repair_output_tests.rs
    │       ├── repair_prompt_tests.rs
    │       ├── repair_tests.rs
    │       └── repair_trace_tests.rs
    └── runtime/
        ├── runtime.rs
        └── tests/runtime_tests.rs
```

---

## 4. Behavior Rules

1. **R-P2-1**: Runtime is the only owner of proposal history, report history, and `repairs_used`.
2. **R-P2-2**: Runtime increments `repairs_used` before each Repair stage invocation.
3. **R-P2-3**: One successful Repair always produces exactly one subsequent full Validate.
4. **R-P2-4**: Repair receives only the current proposal and current report's failed issues.
5. **R-P2-5**: Repair never receives the original proposal separately, prior reports, resolved issues, or older proposal bodies.
6. **R-P2-6**: Validate and Repair receive references to the same immutable `ValidationContext` evidence.
7. **R-P2-7**: An invalid Validate output fails immediately and does not consume repair budget.
8. **R-P2-8**: A failed Repair consumes the already-reserved round and fails the Turn immediately.
9. **R-P2-9**: Exhausted hard-gate failure stops before Extract, Summary, and Commit.
10. **R-P2-10**: Exhausted score-only failure delegates to Phase 3 Choice ranking.
11. **R-P2-11**: With `max_repair_rounds = 0`, Runtime validates once and never invokes Repair.
12. **R-P2-12**: Proposal/report history length never exceeds `max_repair_rounds + 1`.
13. **R-P2-13**: The old original/current proposal and score-only Repair contract is deleted.
14. **R-P2-14**: Pipelines do not write storage or emit stage events directly.

### 4.1 Error Handling

- Empty repaired output returns `PipelineError::EmptyOutput { stage: Repair }`.
- Oversized repaired output returns `InvalidOutput { stage: Repair, code: "repair_proposal_too_long", .. }`.
- Empty Generate output returns `PipelineError::EmptyOutput { stage: Generate }`.
- Oversized Generate output returns `InvalidOutput { stage: Generate, code: "generate_proposal_too_long", .. }`.
- Hard-gate exhaustion returns `ValidationFailed { code: "validation_hard_gate_exhausted", .. }`.
- Choice without an eligible candidate is handled by Phase 3 code `validation_choice_unavailable`.
- Repair prompt, persistence, and LLM failures preserve `stage: Repair`.
- No error path commits a Turn.

### 4.2 Concurrency

- Validate and Repair execute sequentially inside the Runtime loop.
- Repair performs exactly one LLM call through the shared injected `LlmGateway`.
- The loop starts no task and creates no channel or queue.
- No lock is held across prompt loading, prompt rendering, LLM completion, event emission, or storage access.
- All loop state lives in the `run_turn` stack frame and is dropped when the Turn ends.

### 4.3 Observability

- `PipelineRunner` creates a distinct Chain for every Validate and Repair iteration.
- Repair creates one `repair_generation` child Span and passes it to `LlmGateway`.
- Repair metadata includes only bounded counts, versions, rounds, and byte lengths.
- Validate metadata records used and remaining budget on every revalidation.
- Hard-gate exhaustion is returned with a stable code and final proposal version.

---

## 5. Acceptance Criteria

- [ ] `RepairPipeline`, `RepairInput`, and `RepairOutput` match §3.1.
- [ ] Both Validate and Repair use `project_validation_evidence`.
- [ ] Prompt `repair.repair_story` is registered with CSI, RC, and FTI assets.
- [ ] Repair prompt tests assert all variables in §3.3 and exact issue ordering.
- [ ] Repair output tests cover empty, whitespace-only, exact-boundary, oversized, and unchanged valid output.
- [ ] Repair pipeline tests assert one gateway call through the shared gateway.
- [ ] Runtime tests prove budget consumption occurs before Repair.
- [ ] Runtime tests prove each repaired proposal is fully revalidated.
- [ ] Runtime tests prove `max_repair_rounds = 0` invokes Validate once and Repair zero times.
- [ ] Runtime tests prove hard-gate exhaustion skips Extract, Summary, and Commit.
- [ ] Runtime tests prove Validate parse failure does not invoke Repair.
- [ ] Runtime tests prove proposal/report history never exceeds `max_repair_rounds + 1`.
- [ ] Trace tests cover every metadata key in §3.5 without content-bearing metadata.
- [ ] `rg 'original_proposal|scores:.*ValidateScoreResult|RepairPipeline;' crates/aise-core/src/pipeline` returns zero matches.
- [ ] `rg 'pipeline::validate' crates/aise-core/src/pipeline/repair` returns zero matches.
- [ ] `rg 'pipeline::repair' crates/aise-core/src/pipeline/validate` returns zero matches.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.

---

## 6. Out of Scope / Future Work

- Choice ranking and persistence are specified in [Phase 3](./2026-10-16-validate-pipeline-spec-phase-3-gpt.md).
- Selective or incremental revalidation is intentionally excluded.
- Repair diff output or patch application requires a separate design.

---

## 7. References

- Source design: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
- Phase 0: [Contracts and Budget](./2026-10-16-validate-pipeline-spec-phase-0-gpt.md)
- Phase 1: [Validate Evaluator](./2026-10-16-validate-pipeline-spec-phase-1-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
