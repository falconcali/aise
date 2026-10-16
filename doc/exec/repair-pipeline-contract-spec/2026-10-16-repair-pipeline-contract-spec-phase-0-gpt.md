# Proposal Quality Policy Contracts — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-16
> **Status**: Proposed
> **Source Design**: [Generate/Repair Quality Target Contract](../../design/2026-10-16-repair-pipeline-contract-design-gpt.md)
> **Phase**: Phase 0

---

## 1. Goal

Define one bounded, immutable proposal-quality policy and typed repair-feedback contract that Generate, Validate, and Repair can share without cross-stage dependencies.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add shared check and score criterion types under `pipeline::common`.
- Add `ProposalQualityPolicy` as the single runtime representation of all proposal quality targets.
- Replace Validate-private criterion config types with the shared criterion types.
- Add required `target_guidance` to every score criterion.
- Replace the loose issue-kind representation with typed check and score repair feedback.
- Validate all names, descriptions, score ranges, target guidance, counts, and byte bounds during `Runtime::new`.
- Add focused policy, config, serialization, and issue-extraction tests.

### 2.2 Non-Goals

- Does not change Generate, Validate, or Repair prompts.
- Does not change any LLM call or output protocol.
- Does not change repair-budget behavior, Choice ranking, persistence, HTTP APIs, or events.
- Does not add compatibility deserialization for score criteria without `target_guidance`.
- Does not modify frozen reference crates.

### 2.3 Implementation Constraints

- Phases 0–2 form one hard-refactor change and MUST land together.
- Final code MUST NOT retain `ValidateCheckConfig`, `ValidateScoreConfig`, `ValidationCriterionKind`, or a second quality-policy representation.
- Shared contracts MUST live in `pipeline::common`; Generate, Validate, and Repair MUST NOT import one another.
- The policy is immutable after construction and bounded by existing criterion-count and text-size limits.
- Rust source remains comment-free, `mod.rs` remains index-only, and tests remain in `tests/<source>_tests.rs`.
- No fallback, serde alias, optional `target_guidance`, compatibility shim, or dual prompt path is permitted.

---

## 3. Contracts

### 3.1 Shared quality policy

Create `crates/aise-core/src/pipeline/common/proposal_quality.rs`:

```rust
use crate::core::BoundedText;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalCheckCriterion {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalScoreCriterion {
    pub name: String,
    pub description: String,
    pub min_score: f64,
    pub max_score: f64,
    pub target_score: f64,
    pub target_guidance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProposalQualityPolicy {
    checks: Box<[ProposalCheckCriterion]>,
    scores: Box<[ProposalScoreCriterion]>,
}

impl ProposalQualityPolicy {
    pub fn new(
        checks: Vec<ProposalCheckCriterion>,
        scores: Vec<ProposalScoreCriterion>,
    ) -> Self;

    pub fn checks(&self) -> &[ProposalCheckCriterion];
    pub fn scores(&self) -> &[ProposalScoreCriterion];
}
```

`ProposalQualityPolicy` has no mutating API. Stage objects receive `Arc<ProposalQualityPolicy>` in later phases and read criteria only through slice getters.

### 3.2 Typed repair feedback

Move the repair-feedback contract into the same file:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum ValidationIssue {
    Check {
        name: String,
        message: BoundedText,
    },
    Score {
        name: String,
        current_score: f64,
        target_score: f64,
        message: BoundedText,
    },
}
```

The serialized protocol is exact:

```json
[
  {
    "kind": "Check",
    "name": "explicit_constraints",
    "message": "The final paragraph narrates a prohibited outcome."
  },
  {
    "kind": "Score",
    "name": "narrative_quality",
    "current_score": 6.5,
    "target_score": 8.0,
    "message": "The causal transition between the confrontation and retreat is missing."
  }
]
```

`ValidationIssue` does not include passing criteria, score bounds, descriptions, target guidance, aggregate decisions, proposal history, or repair-budget data.

### 3.3 Validation report issue projection

Keep `ValidationCheckResult`, `ValidationScoreResult`, and `ValidationReport` in `pipeline/common/validation.rs`. Replace `failed_issues` with:

```rust
impl ValidationReport {
    pub fn failed_issues(&self) -> Vec<ValidationIssue>;
}
```

The projection is exact:

```text
failed check
  -> ValidationIssue::Check {
       name,
       message = required check issue
     }

score below target
  -> ValidationIssue::Score {
       name,
       current_score = score,
       target_score,
       message = required score issue
     }

passing check or score at/above target
  -> no ValidationIssue
```

The output order is configured check order followed by configured score order.

### 3.4 Validate configuration

Replace the criterion types in `crates/aise-core/src/pipeline/validate/config.rs`:

```rust
use crate::pipeline::common::{
    PipelineError, PipelineStage, ProposalCheckCriterion, ProposalQualityPolicy,
    ProposalScoreCriterion,
};
use serde::Deserialize;

pub const MAX_VALIDATE_CHECKS: usize = 16;
pub const MAX_VALIDATE_SCORES: usize = 16;
pub const MAX_CRITERION_NAME_BYTES: usize = 64;
pub const MAX_CRITERION_DESCRIPTION_BYTES: usize = 2 * 1024;
pub const MAX_TARGET_GUIDANCE_BYTES: usize = 2 * 1024;
pub const MAX_ISSUE_BYTES_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateConfig {
    pub checks: Vec<ProposalCheckCriterion>,
    pub scores: Vec<ProposalScoreCriterion>,
    pub max_issue_bytes: usize,
}

impl ValidateConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
    pub fn quality_policy(&self) -> ProposalQualityPolicy;
}
```

`quality_policy()` clones the already validated criterion vectors into one immutable policy. `Runtime::new` MUST call `PipelineConfig::validate()` before calling `quality_policy()`.

Configuration validation retains every existing bound and additionally enforces:

```text
target_guidance.trim().len() >= 1
target_guidance.trim().len() <= 2048 bytes
```

Use these stable errors:

```text
validate_score_target_guidance_empty
validate_score_target_guidance_too_long
```

Delete `ValidateCheckConfig` and `ValidateScoreConfig`; do not re-export aliases.

### 3.5 Default score guidance

The four default score criteria retain their current names, descriptions, ranges, and targets. Add these non-empty guidance values:

```text
story_goal_completion
  The segment makes visible, causally grounded progress toward the immediate story goal without forcing completion unsupported by the evidence.

player_response
  The segment realizes the pending player contribution in the prose, preserves its essential meaning, and does not invent an unprovided consequential player choice.

character_consistency
  Character actions, knowledge, voice, and immediate intent remain consistent with supplied role evidence and character decisions unless visible events justify a change.

narrative_quality
  The segment uses clear causal and temporal transitions, readable prose, and a natural interaction boundary without relying on abrupt or unsupported developments.
```

Default check criteria remain unchanged.

### 3.6 Visibility and re-exports

Update `pipeline/common/mod.rs`:

```rust
mod proposal_quality;

pub(super) use proposal_quality::{
    ProposalCheckCriterion, ProposalQualityPolicy, ProposalScoreCriterion, ValidationIssue,
};
```

Remove `ValidationCriterionKind` and the old `ValidationIssue` declaration from `validation.rs`. Do not expose these pipeline contracts from `core`.

### 3.7 File layout

```text
crates/aise-core/src/pipeline/
├── common/
│   ├── mod.rs
│   ├── proposal_quality.rs
│   ├── validation.rs
│   └── tests/
│       ├── proposal_quality_tests.rs
│       └── validation_tests.rs
└── validate/
    ├── config.rs
    ├── mod.rs
    └── tests/
        └── config_tests.rs
```

---

## 4. Behavior Rules

1. **R-P0-1**: One `ProposalQualityPolicy` contains every configured check and score in configured order.
2. **R-P0-2**: The policy exposes no mutation method and is not rebuilt during a Turn.
3. **R-P0-3**: Criterion names remain unique across both check and score collections.
4. **R-P0-4**: Every score has finite ordered bounds, an in-range target, and non-empty bounded target guidance.
5. **R-P0-5**: A failed check produces exactly one `ValidationIssue::Check`.
6. **R-P0-6**: A below-target score produces exactly one `ValidationIssue::Score` with the report's current and target values.
7. **R-P0-7**: Passing and at-target criteria produce no repair feedback.
8. **R-P0-8**: `failed_issues()` preserves check order followed by score order.
9. **R-P0-9**: `ValidationIssue` cannot represent score feedback without both current and target scores.
10. **R-P0-10**: No module under `core` imports a quality-policy or validation type.

### 4.1 Error Handling

- Missing score guidance returns `PipelineError::InvalidConfig` with stage `Validate` and code `validate_score_target_guidance_empty`.
- Oversized score guidance returns `PipelineError::InvalidConfig` with stage `Validate` and code `validate_score_target_guidance_too_long`.
- Existing invalid-name, duplicate-name, count, description, numeric, range, and issue-byte errors remain unchanged.
- Invalid configuration is rejected during `Runtime::new`, before any Turn or observation starts.
- `failed_issues()` may use `expect` only for report invariants already guaranteed by strict Validate output parsing.

### 4.2 Concurrency

- Policy construction is synchronous and occurs once during Runtime construction.
- This phase adds no task, queue, channel, lock, I/O, or LLM call.
- Later stage sharing uses `Arc<ProposalQualityPolicy>`; no lock is required.

### 4.3 Observability

- This phase adds no metadata, metric, log, or observation.
- Config errors remain diagnosable through existing typed error fields.
- Criterion text and guidance are prompt content in later phases, not trace metadata.

---

## 5. Acceptance Criteria

- [ ] `ProposalQualityPolicy`, `ProposalCheckCriterion`, and `ProposalScoreCriterion` match §3.1.
- [ ] `ValidationIssue` matches the tagged enum and JSON shapes in §3.2.
- [ ] `ValidateConfig` uses shared criterion types and requires `target_guidance`.
- [ ] Config tests cover empty, whitespace-only, exact-boundary, and oversized target guidance.
- [ ] Default-config tests assert the exact four guidance values in §3.5.
- [ ] Validation tests assert issue variant, current score, target score, message, and ordering.
- [ ] Serialization tests assert that check feedback has no score fields and score feedback has both score fields.
- [ ] `rg 'ValidateCheckConfig|ValidateScoreConfig|ValidationCriterionKind' crates/aise-core crates/aise-service` returns zero matches after all phases.
- [ ] No `Option<String>` or serde default is used to make `target_guidance` backward-compatible.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.

---

## 6. Out of Scope / Future Work

- Generate and Validate policy injection and prompt rendering are specified in [Phase 1](./2026-10-16-repair-pipeline-contract-spec-phase-1-gpt.md).
- Repair feedback rendering and Runtime wiring are specified in [Phase 2](./2026-10-16-repair-pipeline-contract-spec-phase-2-gpt.md).
- Criterion weighting, dynamic policy selection, and per-story policy overrides require a separate source design.

---

## 7. References

- Source design: [Generate/Repair Quality Target Contract](../../design/2026-10-16-repair-pipeline-contract-design-gpt.md)
- Architecture baseline: [Pipeline Framework Design](../../design/2026-10-13-pipeline-framework-design-opus.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Code organization: `doc/agents/guardrails/code-organization.md`
