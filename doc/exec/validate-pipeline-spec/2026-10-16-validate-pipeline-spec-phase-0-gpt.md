# Validate Pipeline Contracts and Budget — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
> **Phase**: Phase 0

---

## 1. Goal

Establish the bounded validation contracts, configuration, persisted evaluation model, and Runtime-owned repair budget required by the Validate/Repair loop.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add Validate/Repair shared evidence, issue, result, and report types under `pipeline::common`.
- Add validated `ValidateConfig`, `RepairConfig`, and `ValidationLoopConfig` contracts.
- Aggregate all three configs under `PipelineConfig` and make `Runtime::new` reject invalid configuration.
- Replace `TurnStatus::Accepted | Rejected` with `TurnStatus::Pass | Choice`.
- Replace the empty `TurnEvaluation` with a bounded persisted evaluation summary.
- Define the final Runtime loop-result contract consumed by Extract and Commit.
- Add dedicated unit tests for all new contracts and config validation.

### 2.2 Non-Goals

- Does not call an LLM from Validate or Repair.
- Does not add Validate or Repair prompt assets.
- Does not implement the Validate/Repair loop, Choice ranking, or Commit wiring.
- Does not validate Extract-produced world state.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0–3 form one hard-refactor change and MUST land together.
- Do not retain `ValidationDecision`, `ValidateScoreResult`, `TurnStatus::Accepted`, `TurnStatus::Rejected`, or the empty `TurnEvaluation`.
- Shared Validate/Repair types MUST live in `pipeline::common`; neither stage may import the other.
- Runtime owns the mutable repair counter. `ValidationLoopConfig` contains only immutable limits.
- Rust source remains comment-free and tests remain in `tests/<source>_tests.rs`.
- No fallback, legacy serde alias, compatibility shim, or dual status path is permitted.

---

## 3. Contracts

### 3.1 Shared validation contracts

Create `crates/aise-core/src/pipeline/common/validation.rs`:

```rust
use crate::core::{
    ActivatedKnowledge, BoundedText, CharacterDecision, FactEntry, NarrativeDirection, PlayerContribution,
    PlayerRoleContext, ResolvedRole, RumorEntry,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ValidationContext<'a> {
    pub story_goal: &'a BoundedText,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
    pub player_role: &'a PlayerRoleContext,
    pub roles: &'a [ResolvedRole],
    pub character_decisions: &'a [CharacterDecision],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ValidationCriterionKind {
    Check,
    Score,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    pub kind: ValidationCriterionKind,
    pub name: String,
    pub message: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationCheckResult {
    pub name: String,
    pub description: String,
    pub passed: bool,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationScoreResult {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub score: f64,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationReport {
    pub proposal_version: u32,
    pub checks: Vec<ValidationCheckResult>,
    pub scores: Vec<ValidationScoreResult>,
}

impl ValidationReport {
    pub fn all_checks_pass(&self) -> bool;
    pub fn all_scores_meet_target(&self) -> bool;
    pub fn failed_issues(&self) -> Vec<ValidationIssue>;
    pub fn unmet_score_count(&self) -> usize;
}
```

`pipeline/common/mod.rs` re-exports these types with `pub(super)` visibility for pipeline modules. No validation type is added to `core`.

### 3.2 Validate configuration

Create `crates/aise-core/src/pipeline/validate/config.rs`:

```rust
use crate::pipeline::common::PipelineError;
use serde::Deserialize;

pub const MAX_VALIDATE_CHECKS: usize = 16;
pub const MAX_VALIDATE_SCORES: usize = 16;
pub const MAX_CRITERION_NAME_BYTES: usize = 64;
pub const MAX_CRITERION_DESCRIPTION_BYTES: usize = 2 * 1024;
pub const MAX_ISSUE_BYTES_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateCheckConfig {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateScoreConfig {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateConfig {
    pub checks: Vec<ValidateCheckConfig>,
    pub scores: Vec<ValidateScoreConfig>,
    pub max_issue_bytes: usize,
}

impl ValidateConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
}
```

`ValidateConfig::default()` MUST use `max_issue_bytes = 2048` and these stable criteria:

```text
checks
  internal_logic
  narrative_consistency
  explicit_constraints
  private_knowledge_boundary

scores, each with min_score = 0.0, max_score = 10.0, target_score = 8.0
  story_goal_completion
  player_response
  character_consistency
  narrative_quality
```

Each default description MUST state one observable criterion and MUST NOT combine multiple names into one rubric. Configuration validation MUST enforce:

```text
1 <= checks.len() + scores.len()
checks.len() <= 16
scores.len() <= 16
1 <= name.len() <= 64 bytes
1 <= description.len() <= 2048 bytes after trim
all names unique across checks and scores
min_score, max_score, target_score are finite
min_score < max_score
min_score <= target_score <= max_score
1 <= max_issue_bytes <= 16384
```

### 3.3 Repair and loop configuration

Create `crates/aise-core/src/pipeline/repair/config.rs`:

```rust
use crate::pipeline::common::PipelineError;
use serde::Deserialize;

pub const MAX_REPAIR_PROPOSAL_BYTES_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairConfig {
    pub max_proposal_bytes: usize,
}

impl RepairConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
}
```

`RepairConfig::default().max_proposal_bytes` is `64 * 1024`. Validation requires `1..=1024 * 1024`.

Add to `crates/aise-core/src/pipeline/common/config.rs`:

```rust
use crate::pipeline::repair::RepairConfig;
use crate::pipeline::validate::ValidateConfig;
use serde::Deserialize;

pub const MAX_REPAIR_ROUNDS_LIMIT: u32 = 8;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationLoopConfig {
    pub max_repair_rounds: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub generate: GenerateConfig,
    pub validate: ValidateConfig,
    pub repair: RepairConfig,
    pub validation_loop: ValidationLoopConfig,
    pub summary: SummaryConfig,
}

impl PipelineConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
}
```

`ValidationLoopConfig::default().max_repair_rounds` is `2`. Validation permits `0..=8`.

### 3.4 Persisted Turn evaluation

Replace the old status and empty evaluation in `crates/aise-core/src/core/turn.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnStatus {
    Pass,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCheckEvaluation {
    pub name: String,
    pub passed: bool,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnScoreEvaluation {
    pub name: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub score: f64,
    pub issue: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnChoiceEvaluation {
    pub selected_proposal_version: u32,
    pub target_gap_mse: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnEvaluation {
    pub proposal_version: u32,
    pub repair_rounds: u32,
    pub checks: Vec<TurnCheckEvaluation>,
    pub scores: Vec<TurnScoreEvaluation>,
    pub choice: Option<TurnChoiceEvaluation>,
}
```

Re-export all five public domain types from `core/mod.rs`. Do not implement `Default` for `TurnEvaluation`; every committed Turn must provide real evaluation data.

Legacy serialized values are handled by a hard schema cut:

```text
"Accepted" -> unsupported after this change
"Rejected" -> unsupported after this change
{} TurnEvaluation -> unsupported after this change
```

No legacy alias or runtime migration branch is allowed. Any durable store introduced later must perform an offline data migration before loading this schema.

### 3.5 Runtime loop result

Add a private Runtime-owned contract in `pipeline/runtime/runtime.rs`:

```rust
struct ValidationLoopResult {
    proposal: String,
    report: ValidationReport,
    repair_rounds: u32,
    status: TurnStatus,
    choice: Option<TurnChoiceEvaluation>,
}
```

The result is bounded by:

```text
reports retained <= max_repair_rounds + 1
proposals retained <= max_repair_rounds + 1
issues per report <= checks.len() + scores.len()
every issue <= max_issue_bytes
every repaired proposal <= max_proposal_bytes
```

### 3.6 Error contract and construction

Extend `PipelineError`:

```rust
#[error("pipeline stage {stage} has invalid configuration ({code}): {message}")]
InvalidConfig {
    stage: PipelineStage,
    code: &'static str,
    message: String,
},

#[error("validation failed at proposal version {proposal_version} ({code}): {message}")]
ValidationFailed {
    code: &'static str,
    proposal_version: u32,
    message: String,
},
```

Change Runtime construction:

```rust
pub fn Runtime::new(
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    pipeline_config: PipelineConfig,
) -> Result<Self, PipelineError>;
```

`Runtime::new` calls `pipeline_config.validate()` before constructing any stage. `AiseEngine::new` propagates this error through the existing `EngineError::Pipeline` conversion.

### 3.7 File layout

```text
crates/aise-core/src/
├── core/
│   └── turn.rs
└── pipeline/
    ├── common/
    │   ├── config.rs
    │   ├── error.rs
    │   ├── validation.rs
    │   └── tests/
    │       └── validation_tests.rs
    ├── repair/
    │   ├── config.rs
    │   └── tests/config_tests.rs
    ├── runtime/
    │   └── runtime.rs
    └── validate/
        ├── config.rs
        └── tests/config_tests.rs
```

---

## 4. Behavior Rules

1. **R-P0-1**: Validate and Repair share only `pipeline::common` contracts; neither module imports the other.
2. **R-P0-2**: Criterion names are unique across both check and score collections.
3. **R-P0-3**: A config with no checks and no scores is rejected at `Runtime::new`.
4. **R-P0-4**: `max_repair_rounds = 0` is valid and means Runtime may never call Repair.
5. **R-P0-5**: Every score range and target is finite and ordered as `min < max` and `min <= target <= max`.
6. **R-P0-6**: `ValidationReport::failed_issues` preserves configured check order followed by configured score order.
7. **R-P0-7**: A failed check or below-target score contributes exactly one `ValidationIssue`; passing entries contribute none.
8. **R-P0-8**: `TurnStatus::Pass` and `TurnStatus::Choice` are the only serializable Turn statuses.
9. **R-P0-9**: All old validation and Turn status types are removed in the same change.
10. **R-P0-10**: No code in `core` imports `pipeline`.

### 4.1 Error Handling

- Invalid Validate config returns `PipelineError::InvalidConfig { stage: Validate, .. }`.
- Invalid Repair config returns `PipelineError::InvalidConfig { stage: Repair, .. }`.
- An excessive repair budget returns code `validation_max_repair_rounds_exceeded` with stage `Validate`.
- Empty criteria return code `validate_criteria_empty`.
- Duplicate names return code `validate_criterion_name_duplicate`.
- Invalid score ranges return code `validate_score_range_invalid`.
- Invalid numeric values return code `validate_score_value_non_finite`.
- Config errors are raised during `AiseEngine::new`, before any Turn starts.

### 4.2 Concurrency

- All contracts and configs are immutable after Runtime construction.
- The repair counter is a local variable in `Runtime::run_turn`; it is not shared, locked, or stored on a pipeline.
- This phase adds no task, channel, lock, or LLM call.

### 4.3 Observability

- Config validation emits no content-bearing log.
- Returned config errors include stable code, stage, and bounded diagnostic message.
- Phase 1 owns evaluator observations; Phase 3 owns final loop outcome metadata.

---

## 5. Acceptance Criteria

- [ ] `pipeline/common/validation.rs` contains the contracts in §3.1.
- [ ] `ValidateConfig`, `RepairConfig`, and `ValidationLoopConfig` match §§3.2–3.3.
- [ ] `PipelineConfig::default()` produces four checks, four scores, a two-round budget, a 2048-byte issue limit, and a 65536-byte repair output limit.
- [ ] Config tests cover every bound and stable error code listed in §4.1.
- [ ] `Runtime::new` returns `Result<Self, PipelineError>` and validates config before stage construction.
- [ ] `TurnStatus` serializes only `"Pass"` and `"Choice"`.
- [ ] `TurnEvaluation` cannot be constructed as an empty struct.
- [ ] `rg 'ValidationDecision|TurnStatus::Accepted|TurnStatus::Rejected|TurnEvaluation \{\}' crates/aise-core crates/aise-service` returns zero matches after all phases.
- [ ] `rg 'pipeline::validate' crates/aise-core/src/pipeline/repair` returns zero matches.
- [ ] `rg 'pipeline::repair' crates/aise-core/src/pipeline/validate` returns zero matches.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.

---

## 6. Out of Scope / Future Work

- Validate execution and strict LLM output parsing are specified in [Phase 1](./2026-10-16-validate-pipeline-spec-phase-1-gpt.md).
- Repair execution and the bounded revalidation loop are specified in [Phase 2](./2026-10-16-validate-pipeline-spec-phase-2-gpt.md).
- Choice ranking, persistence wiring, and end-to-end verification are specified in [Phase 3](./2026-10-16-validate-pipeline-spec-phase-3-gpt.md).

---

## 7. References

- Source design: [Validate Pipeline](../../design/2026-10-16-validate-pipeline-design-gpt.md)
- Architecture baseline: [Pipeline Framework Design](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Code organization: `doc/agents/guardrails/code-organization.md`
