# Choice, Persistence, and Verification — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
> **Phase**: Phase 3

---

## 1. Goal

Complete the Validate/Repair hard refactor with deterministic Choice ranking, persisted Pass/Choice evaluation, final observability, and end-to-end verification.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Rank only hard-gate-passing proposal versions by normalized target-gap mean square.
- Apply deterministic tie-breakers for unmet score count and proposal recency.
- Return a diagnosable failure when no Choice candidate exists.
- Feed the selected proposal into Extract and persist its final report through Commit.
- Persist `TurnStatus::Pass` or `TurnStatus::Choice` and a bounded `TurnEvaluation`.
- Include both committed statuses in prior-story prompt context.
- Record final loop outcome, repair count, selected version, and Choice metric.
- Add complete end-to-end tests and remove every superseded validation/status path.
- Run formatting, clippy with warnings denied, and maintained-crate tests.

### 2.2 Non-Goals

- Does not add criterion weights or an aggregate acceptance threshold.
- Does not use raw total score, raw average score, model arbitration, or last-version fallback.
- Does not commit a Turn after hard-gate exhaustion or stage failure.
- Does not add compatibility deserialization for `Accepted`, `Rejected`, or `{}` evaluation data.
- Does not add a durable-store migration tool.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0–2 are required.
- All phases MUST be reviewed and landed as one hard-refactor change.
- Choice ranking is a pure deterministic function with no LLM or I/O.
- Commit remains the only stage that writes Turn results.
- The selected report and proposal MUST come from the same history entry.
- Old types, branches, fixtures, prompt filters, and serialized status assumptions MUST be deleted, not deprecated.
- No waiver, `#[allow]`, ignored test, or unreviewed snapshot update is permitted.

---

## 3. Contracts

### 3.1 Choice ranking

Create `crates/aise-core/src/pipeline/runtime/validation_choice.rs`:

```rust
use crate::pipeline::common::PipelineError;
use crate::pipeline::runtime::ValidatedProposal;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ChoiceSelection {
    pub history_index: usize,
    pub proposal_version: u32,
    pub target_gap_mse: f64,
    pub unmet_score_count: usize,
}

pub(super) fn select_choice(
    history: &[ValidatedProposal],
) -> Result<ChoiceSelection, PipelineError>;

pub(super) fn target_gap_mse(
    proposal: &ValidatedProposal,
) -> Result<f64, PipelineError>;
```

For every score:

```text
deficit = max(0, (target_score - score) / (max_score - min_score))
```

For one proposal:

```text
target_gap_mse = sum(deficit * deficit) / score_count
```

Eligibility and order are exact:

```text
eligible := report.all_checks_pass()

better candidate :=
  smaller target_gap_mse
  then smaller unmet_score_count
  then larger proposal_version
```

Comparison uses `f64::total_cmp`. The function does not round metrics before comparison or persistence. It ignores every history entry with a failed check.

### 3.2 Ranking failures

`select_choice` returns:

```rust
PipelineError::ValidationFailed {
    code: "validation_choice_unavailable",
    proposal_version: history.last().map(|entry| entry.report.proposal_version).unwrap_or(0),
    message: "no hard-gate-passing proposal is available for choice".to_owned(),
}
```

`target_gap_mse` returns `PipelineError::ValidationFailed` with code `validation_choice_scores_empty` if called with no scores. This is a broken loop invariant because a no-score report with all checks passing must have ended as `Pass`.

### 3.3 Final loop result

Phase 2 completes `ValidationLoopResult` as:

```rust
struct ValidationLoopResult {
    proposal: String,
    report: ValidationReport,
    repair_rounds: u32,
    status: TurnStatus,
    choice: Option<TurnChoiceEvaluation>,
}
```

Pass construction:

```text
proposal = latest proposal
report = latest report
repair_rounds = actual Repair invocations
status = TurnStatus::Pass
choice = None
```

Choice construction:

```text
selection = select_choice(all history)
proposal = history[selection.history_index].proposal
report = history[selection.history_index].report
repair_rounds = actual Repair invocations
status = TurnStatus::Choice
choice = Some(TurnChoiceEvaluation {
  selected_proposal_version: selection.proposal_version,
  target_gap_mse: selection.target_gap_mse
})
```

No proposal or report is cloned more than once when producing the final result. Use ownership transfer from bounded history after ranking.

### 3.4 Persisted evaluation projection

Add a pure Runtime helper:

```rust
fn turn_evaluation(result: &ValidationLoopResult) -> TurnEvaluation;
```

The mapping is:

```text
TurnEvaluation.proposal_version <- result.report.proposal_version
TurnEvaluation.repair_rounds    <- result.repair_rounds
TurnEvaluation.checks           <- result.report.checks, without descriptions
TurnEvaluation.scores           <- result.report.scores, without descriptions
TurnEvaluation.choice           <- result.choice
```

Every persisted issue is the existing bounded `BoundedText`; no issue text is copied through an unbounded intermediate.

The following invariant is mandatory:

```text
status == Pass   <=> choice == None
status == Choice <=> choice != None
```

### 3.5 Commit contract

Change `CommitInput`:

```rust
#[derive(Serialize)]
pub struct CommitInput {
    pub player_contribution: PlayerContribution,
    pub idempotency_key: IdempotencyKey,
    pub turn_segment: TurnSegment,
    pub world_change: WorldChange,
    pub turn_status: TurnStatus,
    pub turn_evaluation: TurnEvaluation,
    pub summary: Change<StorySummary>,
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}
```

`CommitPipeline::execute` sets:

```rust
turn_status: input.turn_status,
turn_evaluation: input.turn_evaluation,
```

It MUST NOT derive, override, or recalculate either field.

Runtime order after validation is:

```text
ValidationLoopResult
  -> ExtractInput.query = result.proposal
  -> Summary
  -> CommitInput.turn_segment = result.proposal
  -> CommitInput.turn_status = result.status
  -> CommitInput.turn_evaluation = turn_evaluation(&result)
```

Extract and Commit always use the selected proposal, including an older Choice candidate.

### 3.6 Prior-story filtering

Replace the old accepted-only filter in `pipeline/common/pipeline_prompt.rs`:

```rust
fn is_committed_story_status(status: TurnStatus) -> bool {
    matches!(status, TurnStatus::Pass | TurnStatus::Choice)
}
```

`turns_story` includes both statuses and preserves existing turn order. There is no rejected status after the migration.

### 3.7 Final loop observation

Create `crates/aise-core/src/pipeline/runtime/runtime_trace.rs`:

```rust
pub(super) fn begin_validation_loop(
    trace: &Trace,
    initial_proposal: &str,
) -> Observation;

pub(super) fn finish_validation_loop(
    observation: Observation,
    result: &Result<ValidationLoopResult, PipelineError>,
);
```

The observation is an explicit child of the Turn trace root:

```text
name = "validation_loop"
kind = ObservationKind::Span
input = capture_content(initial_proposal)
```

Success metadata:

```text
aise.validation.outcome = "pass" | "choice"
aise.validation.final_proposal_version
aise.validation.repair_rounds
aise.validation.checks
aise.validation.failed_checks
aise.validation.scores
aise.validation.unmet_scores
aise.validation.choice.selected_proposal_version
aise.validation.choice.target_gap_mse
```

Choice-only fields are omitted for Pass. Failure observations use the `PipelineError` stable code and `stage = "validate"`. The final proposal/report is captured through `Observation::capture_content`; proposal and issue bodies do not enter metadata.

### 3.8 Required tests

Create or extend dedicated test files with these exact test names:

```text
pipeline/runtime/tests/validation_choice_tests.rs
  target_gap_mse_normalizes_different_score_ranges
  target_gap_mse_ignores_scores_at_or_above_target
  choice_excludes_failed_check_candidates
  choice_prefers_smaller_target_gap_mse
  choice_breaks_metric_tie_by_fewer_unmet_scores
  choice_breaks_full_tie_by_newer_proposal
  choice_rejects_history_without_eligible_candidate
  choice_rejects_empty_score_set

pipeline/runtime/tests/runtime_tests.rs
  runtime_passes_initial_proposal_without_repair
  runtime_repairs_then_passes_new_version
  runtime_revalidates_all_criteria_after_repair
  runtime_zero_budget_chooses_initial_score_only_candidate
  runtime_exhaustion_chooses_best_hard_gate_passing_candidate
  runtime_choice_can_select_older_proposal
  runtime_hard_gate_exhaustion_skips_extract_summary_and_commit
  runtime_validate_contract_error_skips_repair_and_commit
  runtime_repair_error_skips_next_validate_and_commit
  runtime_persists_pass_evaluation
  runtime_persists_choice_evaluation

pipeline/commit/tests/commit_tests.rs
  commit_persists_pass_status_and_evaluation
  commit_persists_choice_status_and_evaluation

pipeline/common/tests/story_context_prompt_tests.rs
  turns_story_includes_pass_and_choice

pipeline/runtime/tests/runtime_trace_tests.rs
  validation_loop_trace_records_pass
  validation_loop_trace_records_choice_metric
  validation_loop_trace_records_hard_gate_exhaustion
```

### 3.9 Forbidden residuals

These patterns MUST have zero matches under maintained crates:

```text
ValidationDecision
ValidateScoreResult
total_score(
select_final_index
original_proposal
TurnStatus::Accepted
TurnStatus::Rejected
TurnEvaluation {}
score_configs: vec![]
pub struct RepairPipeline;
```

### 3.10 Verification commands

Run from the repository root:

```powershell
cargo fmt -p aise-core -p aise-service -- --check
cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings
cargo test -p aise-core -p aise-service
rg "ValidationDecision|ValidateScoreResult|total_score\(|select_final_index|original_proposal" crates/aise-core crates/aise-service
rg "TurnStatus::Accepted|TurnStatus::Rejected|TurnEvaluation \{\}|score_configs: vec!\[\]|pub struct RepairPipeline;" crates/aise-core crates/aise-service
```

Both `rg` commands MUST exit with no matches.

---

## 4. Behavior Rules

1. **R-P3-1**: Pass always selects the latest validated proposal and has no Choice metadata.
2. **R-P3-2**: Choice considers every history entry whose checks all pass, including older versions.
3. **R-P3-3**: A failed check makes a candidate ineligible regardless of numeric scores.
4. **R-P3-4**: Scores at or above target contribute zero deficit; excess score never offsets another criterion's deficit.
5. **R-P3-5**: Different score ranges are normalized before comparison.
6. **R-P3-6**: Metric ties prefer fewer unmet scores, then the newer proposal version.
7. **R-P3-7**: Runtime never makes an additional LLM call to choose a proposal.
8. **R-P3-8**: The selected proposal and selected report move together into Extract and Commit.
9. **R-P3-9**: `repair_rounds` records actual Repair calls, not proposal version minus one by assumption.
10. **R-P3-10**: Commit persists the Runtime-provided status and evaluation atomically with the Turn.
11. **R-P3-11**: Pass and Choice turns are both eligible prior-story text.
12. **R-P3-12**: Validation failure, invalid output, LLM failure, Repair failure, and unavailable Choice produce no Turn record.
13. **R-P3-13**: Old status serialization and validation decision paths are absent.
14. **R-P3-14**: Frozen reference crates remain byte-for-byte unmodified.

### 4.1 Error Handling

- No eligible Choice candidate returns `validation_choice_unavailable`.
- An empty score set reaching Choice returns `validation_choice_scores_empty`.
- Non-finite ranking input is a broken validated-report invariant and returns `validation_choice_score_non_finite`.
- Ranking errors stop before Extract, Summary, and Commit.
- Persistence failures remain `PipelineError::Persistence { stage: Commit, .. }`.
- Tests assert variants and stable codes, not only rendered error strings.

### 4.2 Concurrency

- Ranking and evaluation projection are synchronous pure functions.
- Choice starts no task, performs no I/O, and acquires no lock.
- Commit preserves its existing single atomic store write.
- No event, channel send, observation finish, or I/O occurs while a write lock is held.

### 4.3 Observability

- Every LLM call remains inside its stage observation and shared gateway.
- The `validation_loop` Span covers the complete loop and records final success or failure once.
- Indexed criterion detail remains in Validate observations; final Runtime metadata contains only aggregate counts and Choice fields.
- Choice metric and selected proposal version are recorded both in trace metadata and persisted evaluation.
- All content capture passes through the configured capture policy.

---

## 5. Acceptance Criteria

- [ ] Ranking functions and tie-breakers match §§3.1–3.2 exactly.
- [ ] Every test name in §3.8 exists in a dedicated external test module.
- [ ] Runtime tests prove an older hard-gate-passing version can win Choice.
- [ ] Runtime tests prove a high score cannot offset another criterion's target deficit.
- [ ] Runtime tests prove hard-gate failure can never produce Choice or Commit.
- [ ] Commit tests prove status/evaluation are passed through without recalculation.
- [ ] Prior-story tests include both Pass and Choice in order.
- [ ] Trace tests assert final outcome, repair count, selected version, and Choice metric.
- [ ] Legacy serialized `Accepted`, `Rejected`, and empty evaluation fixtures are removed.
- [ ] Both forbidden-pattern searches in §3.10 return no matches.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.
- [ ] `cargo fmt -p aise-core -p aise-service -- --check` exits `0`.
- [ ] `cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings` exits `0`.
- [ ] `cargo test -p aise-core -p aise-service` exits `0`.
- [ ] The source design status is changed from `Draft` to `Accepted` only after every criterion above passes.

---

## 6. Out of Scope / Future Work

- Criterion weighting or aggregate thresholds require a new source design.
- Provider-native structured output belongs to an `LlmGateway` design.
- Deterministic post-Extract world-state validation requires a separate pipeline design.
- Durable-store migration tooling is deferred until a persistent backend exists.

---

## 7. References

- Source design: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
- Phase 0: [Contracts and Budget](./2026-10-16-validate-pipeline-spec-phase-0-gpt.md)
- Phase 1: [Validate Evaluator](./2026-10-16-validate-pipeline-spec-phase-1-gpt.md)
- Phase 2: [Repair and Revalidation Loop](./2026-10-16-validate-pipeline-spec-phase-2-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
- Toolchain requirements: `doc/agents/guardrails/toolchain.md`
