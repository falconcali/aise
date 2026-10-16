# Validate Evaluator — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
> **Phase**: Phase 1

---

## 1. Goal

Implement Validate as one observed `LlmGateway` evaluation that strictly maps configured checks and scores into a bounded `ValidationReport`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Replace the placeholder Validate implementation with an injected, configured pipeline.
- Render all generation evidence, prior-story context, current proposal, and criteria through CSI/RC/FTI prompt assets.
- Add one Validate LLM call through `LlmGateway`.
- Parse strict JSON and align every returned item with configuration by name.
- Reject unknown, missing, duplicate, non-finite, out-of-range, or semantically inconsistent results.
- Add a Validate evaluator observation with bounded metadata.
- Add focused config, prompt, parser, trace, and pipeline tests.

### 2.2 Non-Goals

- Does not call Repair or make budget decisions.
- Does not return `Pass`, `Choice`, or any other Runtime decision.
- Does not compare proposal history.
- Does not modify a proposal.
- Does not validate Extract output or committed world state.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phase 0 is required.
- Phases 0–3 MUST land as one hard-refactor change.
- Validate MUST NOT import Repair or Runtime.
- The pipeline receives all Turn-scoped evidence through `ValidateInput`.
- Prompt text lives only in prompt assets; Rust contains IDs, variable names, and assembly logic.
- The LLM returns observations, not decisions or aggregate scores.
- No provider is called directly; no retry or parser fallback is added.

---

## 3. Contracts

### 3.1 Validate pipeline

Replace `crates/aise-core/src/pipeline/validate/validate.rs` with:

```rust
use crate::core::{StoryContext, TurnControl};
use crate::llm::LlmGateway;
use crate::persistence::Store;
use crate::pipeline::common::{
    Pipeline, PipelineError, PipelineStage, ValidationContext, ValidationReport,
};
use crate::pipeline::validate::ValidateConfig;
use crate::prompt::Prompt;
use crate::trace::Observation;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ValidateInput<'a> {
    pub context: ValidationContext<'a>,
    pub proposal: &'a str,
    pub proposal_version: u32,
    pub repair_rounds_used: u32,
    pub repair_rounds_remaining: u32,
}

#[derive(Serialize)]
pub struct ValidateOutput {
    pub report: ValidationReport,
}

pub struct ValidatePipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: ValidateConfig,
}

impl ValidatePipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: ValidateConfig,
    ) -> Self;
}

impl<'a> Pipeline<ValidateInput<'a>> for ValidatePipeline {
    type Output = ValidateOutput;

    fn stage(&self) -> PipelineStage;

    async fn execute(
        &self,
        input: ValidateInput<'a>,
        story_ctx: &StoryContext,
        control: &TurnControl,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError>;
}
```

`execute` has this orchestration shape:

```rust
let evaluator_observation = validate_trace::begin_evaluator(observation, &input);
let result = self.evaluate(&input, story_ctx, control, &evaluator_observation).await;
validate_trace::finish_evaluator(evaluator_observation, &input, &result);
result.map(|report| ValidateOutput { report })
```

### 3.2 LLM output protocol

The only accepted JSON shape is:

```json
{
  "checks": [
    {
      "name": "internal_logic",
      "passed": true,
      "issue": null
    }
  ],
  "scores": [
    {
      "name": "story_goal_completion",
      "score": 8.5,
      "issue": null
    }
  ]
}
```

Define private DTOs in `validate_report.rs`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateLlmOutputDto {
    checks: Vec<ValidateCheckDto>,
    scores: Vec<ValidateScoreDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateCheckDto {
    name: String,
    passed: bool,
    issue: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateScoreDto {
    name: String,
    score: f64,
    issue: Option<String>,
}

pub(super) fn parse_validation_report(
    raw: &str,
    proposal_version: u32,
    config: &ValidateConfig,
) -> Result<ValidationReport, PipelineError>;
```

The DTO MUST NOT contain descriptions, ranges, targets, decisions, totals, weights, or proposal text. `parse_validation_report` restores descriptions, ranges, and targets from `ValidateConfig` and emits results in configured order, regardless of LLM array order.

### 3.3 Prompt contract

Register:

```toml
[[prompts]]
id = "validate.evaluate_story"
csi = "csi/validate-evaluate-story.md.j2"
rc = "rc/validate-evaluate-story.md.j2"
fti = "fti/validate-evaluate-story.md.j2"
```

Create `validate_prompt.rs` with:

```rust
pub(super) async fn evaluate_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &ValidateInput<'_>,
    config: &ValidateConfig,
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
check_criteria
score_criteria
max_issue_bytes
```

Use `pipeline_prompt::story_summary`, `pipeline_prompt::recent_story`, `pipeline_prompt::load_story_opening`, `pipeline_prompt::narrative_directions`, and `pipeline_prompt::story_constraints`. Evidence collections retain these boundaries:

```text
activated facts != retrieved facts
activated rumors != retrieved rumors
resolved roles != character decisions
character decisions = viewpoint and writing guidance, never committed world fact
```

CSI MUST define an evaluator that cannot rewrite the story or invent evidence. RC MUST keep all runtime sections in the untrusted user-message layer, use plain headings, and omit optional sections when their content is empty. FTI MUST require the exact JSON protocol from §3.2, one result per configured criterion, actionable issues, and no surrounding prose.

### 3.4 LLM call

Create `validate_llm.rs`:

```rust
pub(super) async fn evaluate_story(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError>;
```

It performs exactly one:

```rust
gateway
    .complete(LlmCompletionSpec { messages }, control, observation)
    .await
```

Validate uses the returned `content` unchanged as input to `parse_validation_report`.

### 3.5 Strict report construction

For each configured check:

```text
passed = true  => issue must be null
passed = false => issue must be non-empty after trim and <= max_issue_bytes
```

For each configured score:

```text
score must be finite
min_score <= score <= max_score
score >= target_score => issue must be null
score < target_score  => issue must be non-empty after trim and <= max_issue_bytes
```

Issue construction is:

```rust
BoundedText::try_new(issue.trim(), "validation_issue", config.max_issue_bytes)
```

Names are exact, case-sensitive matches. The returned check-name set and score-name set MUST each exactly equal their configured sets. A name configured as a check cannot be returned as a score, and vice versa.

### 3.6 Trace contract

Create `validate_trace.rs`:

```rust
pub(super) fn begin_evaluator(
    observation: &Observation,
    input: &ValidateInput<'_>,
) -> Observation;

pub(super) fn finish_evaluator(
    observation: Observation,
    input: &ValidateInput<'_>,
    result: &Result<ValidationReport, PipelineError>,
);
```

The child observation has:

```text
name = "validate_evaluator"
kind = ObservationKind::Span
input = capture_content((proposal_version, proposal))
```

Success metadata uses these static attributes:

```text
aise.validate.proposal_version
aise.validate.checks.total
aise.validate.checks.passed
aise.validate.checks.failed
aise.validate.scores.total
aise.validate.scores.met
aise.validate.scores.unmet
aise.validate.repair_rounds_used
aise.validate.repair_rounds_remaining
```

For criterion index `N`, add bounded indexed metadata:

```text
aise.validate.check.N.name
aise.validate.check.N.passed
aise.validate.score.N.name
aise.validate.score.N.value
aise.validate.score.N.target
```

The full report is captured only through `Observation::capture_content`. Raw prompt text, proposal text, and issue text MUST NOT appear in metadata.

### 3.7 Stable errors

`parse_validation_report` uses `PipelineError::InvalidOutput { stage: Validate, .. }` and these codes:

```text
validate_output_invalid_json
validate_check_count_mismatch
validate_score_count_mismatch
validate_criterion_duplicate
validate_criterion_unknown
validate_criterion_kind_mismatch
validate_score_non_finite
validate_score_out_of_range
validate_issue_missing
validate_issue_unexpected
validate_issue_too_long
```

Prompt, storage, and LLM failures use the existing `Prompt`, `Persistence`, and `Llm` variants with `stage: Validate`.

### 3.8 File layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/validate-evaluate-story.md.j2
│   ├── fti/validate-evaluate-story.md.j2
│   ├── rc/validate-evaluate-story.md.j2
│   └── index.toml
└── src/pipeline/validate/
    ├── config.rs
    ├── mod.rs
    ├── validate.rs
    ├── validate_llm.rs
    ├── validate_prompt.rs
    ├── validate_report.rs
    ├── validate_trace.rs
    └── tests/
        ├── config_tests.rs
        ├── validate_prompt_tests.rs
        ├── validate_report_tests.rs
        ├── validate_tests.rs
        └── validate_trace_tests.rs
```

---

## 4. Behavior Rules

1. **R-P1-1**: Each Validate execution performs exactly one prompt render and one `LlmGateway::complete` call.
2. **R-P1-2**: Validate receives the same goal, player contribution, directions, constraints, knowledge, roles, and character decisions used by Generate.
3. **R-P1-3**: Validate evaluates only `input.proposal`; no original or prior proposal is rendered.
4. **R-P1-4**: The LLM never returns or controls a Runtime decision.
5. **R-P1-5**: Returned criteria are matched by exact name and materialized in configured order.
6. **R-P1-6**: Every configured item appears exactly once and no unconfigured item is accepted.
7. **R-P1-7**: A contract or JSON failure stops the Turn and never becomes a repair issue.
8. **R-P1-8**: A failed check or below-target score is a successful Validate output when its issue is valid.
9. **R-P1-9**: Character decisions are labeled as private viewpoint or writing guidance, not public facts.
10. **R-P1-10**: Validate does not mutate evidence, proposal, `StoryContext`, config, or storage.
11. **R-P1-11**: Prompt content and output content follow the configured observation capture policy.
12. **R-P1-12**: The old always-`Accept` implementation and `total_score` helper are deleted.

### 4.1 Error Handling

- JSON syntax, unknown fields, surrounding prose, and wrong field types return `validate_output_invalid_json`.
- Unknown, missing, duplicate, or wrong-kind criteria return the corresponding stable code in §3.7.
- `NaN`, positive infinity, negative infinity, and out-of-range values are rejected before report construction.
- Empty strings do not satisfy a required issue.
- Any non-null issue on a passing/met item is rejected, including an empty string.
- Error messages may include criterion names but MUST NOT include proposal or issue bodies.

### 4.2 Concurrency

- Validate performs no fan-out and starts no task.
- The LLM call goes through the injected shared `LlmGateway`.
- No lock or guard is held across prompt rendering, storage access, or LLM completion.
- Validate stores no Turn-scoped state on `self`.

### 4.3 Observability

- The stage Chain remains owned by `PipelineRunner`.
- `validate_evaluator` is a child Span under the explicitly passed stage observation.
- `LlmGateway::complete` receives `&evaluator_observation`.
- Failure observations contain stable `code`, `failure_kind`, `stage = "validate"`, and message.
- Criterion metadata is bounded by Phase 0 criterion-count and name-length limits.

---

## 5. Acceptance Criteria

- [ ] `ValidatePipeline` and `ValidateInput` match §3.1.
- [ ] Prompt `validate.evaluate_story` is registered with CSI, RC, and FTI assets.
- [ ] Prompt tests assert every variable in §3.3 and the separation of activated/retrieved knowledge.
- [ ] Prompt tests assert character decisions are labeled as non-factual viewpoint guidance.
- [ ] Parser tests cover valid reordered items and every error code in §3.7.
- [ ] Parser tests cover finite boundaries exactly at `min_score`, `target_score`, and `max_score`.
- [ ] Parser tests reject passing/met items with non-null issues.
- [ ] Parser tests reject failed/unmet items with null, empty, whitespace-only, or oversized issues.
- [ ] Pipeline tests assert exactly one gateway call and no direct provider call.
- [ ] Trace tests assert all static metadata keys and indexed per-item metadata.
- [ ] `rg 'ValidationDecision|total_score|original_proposal' crates/aise-core/src/pipeline/validate` returns zero matches.
- [ ] `rg 'pipeline::repair|pipeline::runtime' crates/aise-core/src/pipeline/validate` returns zero matches.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.

---

## 6. Out of Scope / Future Work

- Repair prompt and output validation are specified in [Phase 2](./2026-10-16-validate-pipeline-spec-phase-2-gpt.md).
- Runtime Choice ranking and persisted evaluation are specified in [Phase 3](./2026-10-16-validate-pipeline-spec-phase-3-gpt.md).
- Provider-native structured output requires a separate `LlmGateway` design.

---

## 7. References

- Source design: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
- Phase 0: [Contracts and Budget](./2026-10-16-validate-pipeline-spec-phase-0-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
