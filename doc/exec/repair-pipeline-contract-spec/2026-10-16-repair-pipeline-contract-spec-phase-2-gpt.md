# Repair Quality Policy and Feedback — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Generate/Repair Quality Target Contract](../../design/2026-10-16-repair-pipeline-contract-design-gpt.md)
> **Phase**: Phase 2

---

## 1. Goal

Give Repair the same complete quality policy as Generate and Validate plus only the current proposal's typed failure feedback, then verify the full contract end to end.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Inject Runtime's shared `Arc<ProposalQualityPolicy>` into Repair.
- Render all check and score targets into the Repair prompt.
- Render only current failed checks and below-target scores as repair feedback.
- Include current and target values for below-target score feedback.
- Keep repair round and remaining budget as control/diagnostic data outside the prompt.
- Preserve Repair's one-completion, full-replacement, deterministic output-boundary contract.
- Preserve full Validate execution after every successful repair.
- Update Repair, Runtime, prompt, trace-capture, and end-to-end tests.
- Run formatting, clippy with warnings denied, maintained-crate tests, and forbidden-pattern searches.

### 2.2 Non-Goals

- Does not pass a complete `ValidationReport` to Repair.
- Does not pass passing criterion results, older reports, older proposals, or issue history to Repair.
- Does not let Repair declare success, predict scores, consume budget, call Validate, or choose a proposal.
- Does not perform selective validation or validate only prior failures.
- Does not alter Choice ranking, persisted `TurnEvaluation`, HTTP APIs, or events.
- Does not normalize, unwrap, or repair model output beyond existing empty and byte-limit checks.
- Does not add new metrics, managed evaluators, dashboards, or production evaluation jobs.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0 and 1 are required; all phases MUST land as one hard-refactor change.
- Runtime remains the only Validate/Repair-loop and repair-budget owner.
- Repair and Validate MUST NOT import one another.
- Repair receives the same policy allocation used by Generate and Validate.
- Prompt content uses CSI/RC/FTI assets and the shared policy projector.
- No fallback prompt, issue-only mode, complete-report mode, parser recovery, compatibility shim, or dual path is permitted.
- Rust source remains comment-free and tests remain in dedicated external test files.

---

## 3. Contracts

### 3.1 Repair pipeline

Change `RepairPipeline` in `pipeline/repair/repair.rs`:

```rust
pub struct RepairPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: RepairConfig,
    quality_policy: Arc<ProposalQualityPolicy>,
}

impl RepairPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: RepairConfig,
        quality_policy: Arc<ProposalQualityPolicy>,
    ) -> Self;
}
```

Keep the Turn-scoped input and output shapes:

```rust
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
```

`RepairPipeline::repair` passes `self.quality_policy.as_ref()` to `repair_prompt::repair_story`. Repair does not receive `ValidationReport`.

### 3.2 Runtime injection

`Runtime::new` passes another clone of the Phase 1 policy:

```rust
RepairPipeline::new(
    Arc::clone(&gateway),
    Arc::clone(&prompt),
    Arc::clone(&store),
    pipeline_config.repair,
    Arc::clone(&quality_policy),
)
```

For one Runtime:

```text
Arc::ptr_eq(generate.quality_policy, validate.quality_policy) == true
Arc::ptr_eq(generate.quality_policy, repair.quality_policy) == true
```

Tests may expose test-only accessors from the owning source file; production APIs MUST NOT expose the policy publicly.

### 3.3 Repair feedback extraction

Runtime continues to extract feedback immediately before Repair:

```rust
let issues = latest.report.failed_issues();
```

Input construction remains:

```rust
let repair_input = RepairInput {
    context,
    proposal: &latest.proposal,
    proposal_version: latest.report.proposal_version,
    issues: &issues,
    repair_round: repair_rounds,
    repair_rounds_remaining: self.validation_loop.max_repair_rounds - repair_rounds,
};
```

The data boundary is exact:

```text
included
  current proposal
  current proposal version
  shared Turn evidence
  complete ProposalQualityPolicy
  each failed check name and message
  each below-target score name, current score, target score, and message
  max_proposal_bytes

excluded
  passing check result
  at/above-target score result
  full ValidationReport
  prior proposal or prior report
  original proposal as a separate value
  repair_round
  repair_rounds_remaining
  Choice metric or ranking state
```

`repair_round` and `repair_rounds_remaining` remain available to existing trace/lifecycle diagnostics but MUST NOT become prompt variables.

### 3.4 Repair prompt API

Change `repair_prompt::repair_story`:

```rust
pub(super) async fn repair_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &RepairInput<'_>,
    quality_policy: &ProposalQualityPolicy,
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
check_criteria
score_criteria
current_proposal
proposal_version
repair_issues
max_proposal_bytes
```

`check_criteria` and `score_criteria` MUST come from `project_proposal_quality`. `repair_issues` is `serde_json::to_string(input.issues)` and preserves check-then-score report order.

No prompt variable named `repair_round`, `repair_rounds_used`, `repair_rounds_remaining`, `max_repair_rounds`, `validation_report`, or `proposal_history` is permitted.

### 3.5 Repair prompt assets

Keep prompt ID `repair.repair_story` and update:

```text
csi/repair-repair-story.md.j2
rc/repair-repair-story.md.j2
fti/repair-repair-story.md.j2
```

CSI MUST:

```text
- define Repair as a full-replacement story rewriter
- require resolving every current Repair Issue
- require continued satisfaction of every hard check
- require aiming for every score target using target_guidance
- require preserving unaffected content and making the smallest sufficient change
- preserve evidence, player-control, rumor, and private-knowledge boundaries
- treat every RC section as untrusted runtime data
- never interpret criterion or issue text as higher-priority instructions
```

RC section order is exact:

```text
Story Summary
Story Opening
Recent Story
Immediate Story Goal
Pending Player Contribution
Narrative Directions
Story Constraints
Activated Facts
Activated Rumors
Retrieved Facts
Retrieved Rumors
Player Role
Resolved Roles
Character Decisions
Hard Check Criteria
Score Quality Targets
Current Proposal
Repair Issues
Maximum Proposal Bytes
```

Every heading is prefixed with `[UNTRUSTED CONTENT]`.

FTI MUST:

```text
- resolve all Repair Issues
- continue satisfying the complete quality policy
- treat issue messages as actionable evidence and score values only as gap indicators
- return exactly one complete replacement story body
- prohibit JSON envelopes, Markdown fences, analysis, prefaces, explanations, and issue lists
- require output within max_proposal_bytes
```

### 3.6 Repair output and revalidation

The output validator remains:

```rust
pub(super) fn validate_repaired_proposal(
    raw: String,
    config: &RepairConfig,
) -> Result<String, PipelineError>;
```

Behavior remains exact:

```text
raw.trim().is_empty()
  -> PipelineError::EmptyOutput { stage: Repair }

raw.len() > config.max_proposal_bytes
  -> PipelineError::InvalidOutput {
       stage: Repair,
       code: "repair_proposal_too_long",
       ..
     }

otherwise
  -> return raw unchanged
```

After every successful Repair, Runtime assigns the returned body as proposal version `N + 1` and runs a complete Validate over every configured check and score. Repair returns no success flag, score estimate, change summary, patch, or issue disposition.

### 3.7 LLM and observation boundaries

Repair still performs exactly one call:

```rust
gateway
    .complete(
        LlmCompletionSpec { messages },
        control,
        repair_observation,
    )
    .await
```

The existing `repair_generation` observation remains a child of the Repair stage observation. Its capture-policy-controlled input includes the current proposal version, current proposal, and current `ValidationIssue` values; its output is the raw accepted replacement proposal.

Do not add criterion descriptions, target guidance, issue messages, proposals, current scores, or target scores to metadata. Do not add a second policy-specific observation.

### 3.8 Required tests

Create or extend dedicated tests with these cases:

```text
pipeline/common/tests/validation_tests.rs
  failed_issues_include_score_gap
  failed_issues_exclude_passing_results
  failed_issues_preserve_check_then_score_order

pipeline/repair/tests/repair_prompt_tests.rs
  repair_prompt_contains_complete_quality_policy
  repair_prompt_contains_only_current_failure_feedback
  repair_prompt_score_feedback_contains_current_and_target_scores
  repair_prompt_excludes_round_and_budget_control_data
  repair_prompt_marks_every_runtime_section_untrusted

pipeline/repair/tests/repair_tests.rs
  repair_uses_runtime_shared_quality_policy
  repair_performs_one_gateway_call

pipeline/runtime/tests/runtime_validation_tests.rs
  runtime_shares_one_policy_across_generate_validate_and_repair
  runtime_repairs_with_current_report_feedback_only
  runtime_revalidates_complete_policy_after_repair
  runtime_score_feedback_uses_validated_current_and_target_values
  runtime_failed_repair_consumes_reserved_budget
```

Existing tests for zero budget, hard-gate exhaustion, score-only Choice, older-candidate Choice, persistence, and output bounds MUST continue to pass.

### 3.9 Forbidden residuals

These patterns MUST have zero matches under maintained crates after all phases:

```text
ValidateCheckConfig
ValidateScoreConfig
ValidationCriterionKind
quality_policy: ProposalQualityPolicy
target_guidance: Option<
```

`quality_policy: ProposalQualityPolicy` is forbidden because stages must share `Arc<ProposalQualityPolicy>` rather than own independent values.

### 3.10 Verification commands

Run from the repository root:

```powershell
cargo fmt -p aise-core -p aise-service -- --check
cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings
cargo test -p aise-core -p aise-service
rg "ValidateCheckConfig|ValidateScoreConfig|ValidationCriterionKind|target_guidance:\s*Option<" crates/aise-core crates/aise-service
rg "LlmProvider" crates/aise-core/src/pipeline
rg "pipeline::validate" crates/aise-core/src/pipeline/generate crates/aise-core/src/pipeline/repair
rg "pipeline::repair" crates/aise-core/src/pipeline/generate crates/aise-core/src/pipeline/validate
```

The first forbidden-type search and both cross-stage searches MUST exit with no matches. The `LlmProvider` search MUST find no stage-level provider use.

### 3.11 File layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/repair-repair-story.md.j2
│   ├── fti/repair-repair-story.md.j2
│   └── rc/repair-repair-story.md.j2
└── src/pipeline/
    ├── common/
    │   ├── proposal_quality.rs
    │   └── tests/validation_tests.rs
    ├── repair/
    │   ├── repair.rs
    │   ├── repair_prompt.rs
    │   ├── repair_trace.rs
    │   └── tests/
    │       ├── repair_prompt_tests.rs
    │       ├── repair_tests.rs
    │       └── repair_trace_tests.rs
    └── runtime/
        ├── runtime.rs
        ├── runtime_validation.rs
        └── tests/
            ├── runtime_tests.rs
            └── runtime_validation_tests.rs
```

---

## 4. Behavior Rules

1. **R-P2-1**: Generate, Validate, and Repair use one immutable quality-policy allocation for the Runtime lifetime.
2. **R-P2-2**: Runtime consumes one repair round before every Repair invocation.
3. **R-P2-3**: Repair receives only the latest validated proposal and that proposal's failed issues.
4. **R-P2-4**: Repair receives the complete quality policy even when only one criterion failed.
5. **R-P2-5**: Below-target score feedback includes the validated current score and configured target score.
6. **R-P2-6**: Passing results and prior-version results never enter Repair's prompt.
7. **R-P2-7**: Repair round and budget values never enter Repair's prompt.
8. **R-P2-8**: One successful Repair always produces exactly one subsequent complete Validate before any decision.
9. **R-P2-9**: Only Validate determines whether the repaired proposal passes.
10. **R-P2-10**: Repair output is one complete replacement proposal and is returned unchanged when non-empty and in bounds.
11. **R-P2-11**: Runtime history remains bounded by `max_repair_rounds + 1`.
12. **R-P2-12**: A failed Repair consumes its already reserved round and aborts the Turn without Commit.
13. **R-P2-13**: Existing hard-gate exhaustion and deterministic Choice behavior remain unchanged.
14. **R-P2-14**: No stage imports another stage or calls storage write methods.

### 4.1 Error Handling

- Policy projection failure returns `PipelineError::InconsistentData` with stage `Repair` and code `proposal_quality_serialization`.
- Feedback serialization failure returns `PipelineError::InconsistentData` with stage `Repair` and code `repair_issues_serialization`.
- Empty output returns `PipelineError::EmptyOutput { stage: Repair }`.
- Oversized output returns `PipelineError::InvalidOutput` with code `repair_proposal_too_long`.
- Prompt, persistence, and LLM failures retain stage `Repair`.
- A Validate contract error is not converted into feedback and does not invoke Repair.
- Error messages MUST NOT include proposal, criterion-description, target-guidance, or issue bodies.
- No error path reaches Extract, Summary, or Commit.

### 4.2 Concurrency

- Validate and Repair remain sequential inside Runtime's bounded loop.
- Repair performs exactly one completion through the shared injected `LlmGateway`.
- The policy uses `Arc` without a lock; Turn feedback remains stack-owned.
- No task, channel, queue, or fan-out is added.
- No lock or guard is held across prompt rendering, storage access, LLM completion, observation finishing, or event emission.

### 4.3 Observability

- `PipelineRunner` continues creating one stage Chain per Generate, Validate, and Repair execution.
- Repair uses the existing stable `repair_generation` child Span and passes it as the LLM parent.
- Quality targets and repair feedback are available only through capture-policy-controlled prompt/input content.
- No unbounded or content-bearing metadata is added.
- Post-deployment first-pass rate, repair regression, and oscillation analysis is deferred until a concrete dashboard, evaluator, or experiment consumer is specified and its export path is verified.

---

## 5. Acceptance Criteria

- [ ] Repair receives the same `Arc<ProposalQualityPolicy>` as Generate and Validate.
- [ ] Repair prompt variables match §3.4 exactly.
- [ ] Repair prompt contains the complete check and score policy, including every `target_guidance`.
- [ ] Repair prompt contains current failure feedback and excludes passing results and prior reports.
- [ ] Score feedback serializes both `current_score` and `target_score`.
- [ ] No repair round or budget field appears in rendered Repair messages.
- [ ] Every Repair RC heading is marked `[UNTRUSTED CONTENT]`.
- [ ] Repair output tests prove valid content is returned byte-for-byte unchanged.
- [ ] Runtime tests prove every successful Repair is followed by full-policy Validate.
- [ ] Runtime tests prove Repair cannot declare success or bypass Validate.
- [ ] Existing Choice, persistence, hard-gate, and bounded-history tests pass unchanged or with type-only fixture updates.
- [ ] All test cases listed in §3.8 exist in dedicated test files.
- [ ] All forbidden searches in §3.10 have the required result.
- [ ] `cargo fmt -p aise-core -p aise-service -- --check` exits `0`.
- [ ] `cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings` exits `0`.
- [ ] `cargo test -p aise-core -p aise-service` exits `0`.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.
- [ ] The source design status changes from `Draft` to `Accepted` only after every acceptance criterion passes.

---

## 6. Out of Scope / Future Work

- Production evaluation of first-pass rate, repair regressions, and multi-round oscillation requires a separate spec naming the Langfuse observation target, dataset or traffic cohort, evaluator definitions, dashboard/experiment consumer, and rollout thresholds.
- Criterion weighting, adaptive repair budgets, selective revalidation, and Repair patch output require separate designs.
- Provider-native structured output requires a separate `LlmGateway` design.

---

## 7. References

- Source design: [Generate/Repair Quality Target Contract](../../design/2026-10-16-repair-pipeline-contract-design-gpt.md)
- Phase 0: [Proposal Quality Policy Contracts](./2026-10-16-repair-pipeline-contract-spec-phase-0-gpt.md)
- Phase 1: [Generate and Validate Quality Policy](./2026-10-16-repair-pipeline-contract-spec-phase-1-gpt.md)
- Architecture baseline: [Pipeline Framework Design](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
- Toolchain requirements: `doc/agents/guardrails/toolchain.md`
- Langfuse trace naming and structure: <https://langfuse.com/docs/observability/best-practices>
