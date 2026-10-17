# Extract Pipeline and Dynamic Knowledge Store — Spec (Phase 2: World Knowledge and Role Memory Extraction)

> **Model**: Gemini 4 Argon
> **Date**: 2026-10-17
> **Status**: Proposed
> **Source Design**: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
> **Phase**: Phase 2

---

## 1. Goal

Implement two-phase dynamic knowledge extraction in `ExtractPipeline`: concurrently run `evaluate_narrative` and `extract_world_knowledge` (full `Fact` `Add`/`Update`, full `Rumor` `Add`/`Update`/`Remove`, and `roles_for_memory` selection with LLM-generated `ActivationRule`), then conditionally run bounded-concurrent per-role `extract_role_memory` (`Memory` `Add`/`Update`), and return the validated `KnowledgeChange` for atomic commit.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Register prompts `extract.extract_world_knowledge` and `extract.extract_role_memory` in `crates/aise-core/assets/prompts/index.toml` with CSI, RC, and FTI templates (including complete `ActivationRule` schema and semantics instructions).
- Create `crates/aise-core/src/pipeline/extract/extract_knowledge.rs` for input view projection, deterministic validation of world knowledge and role memory proposals, deterministic ID allocation for added entries, and `KnowledgeChange` assembly.
- Extend `extract_prompt.rs`, `extract_llm.rs`, and `extract_trace.rs` with `extract_world_knowledge` and `extract_role_memory` support.
- Update `ExtractPipeline::execute` to run `evaluate_narrative` and `extract_world_knowledge` concurrently via `tokio::try_join!`, followed by bounded-concurrent `extract_role_memory` across `roles_for_memory` (`buffered(self.config.max_memory_concurrency.get())`).
- Wire `extract_output.knowledge_change` into `CommitInput.knowledge_change` in `Runtime::run_turn`.
- Add unit and scenario tests covering `Fact` evolution updates, `Rumor` add/update/remove, `Memory` perspective isolation (absent events, others' inner thoughts, secret dialogues, false rumors), `ActivationRule` validation, and deterministic ID generation.

### 2.2 Non-Goals

- Does not allow `Fact` removal (`Remove` is forbidden for `Fact`).
- Does not allow `Memory` removal (`Remove` is forbidden for `Memory`).
- Does not introduce code-level structured perception state or an `evidence` field on `Memory`.
- Does not pass any role's private `memories` into `extract_world_knowledge` or across different roles in `extract_role_memory`.
- Does not modify frozen reference crates (`crates/aise` and `crates/aise-server`).

### 2.3 Implementation Constraints (for code generation)

- Requires [Phase 0](./2026-10-17-extract-pipeline-spec-phase-0-gemini.md) and [Phase 1](./2026-10-17-extract-pipeline-spec-phase-1-gemini.md).
- `ExtractPipeline` MUST NOT write to `Store`; all mutations are returned in `ExtractOutput.knowledge_change` and committed by `CommitPipeline` (`R-AISE-05`).
- Per-role memory extraction fan-out MUST be bounded by `ExtractConfig.max_roles_for_memory` and `ExtractConfig.max_memory_concurrency`, and every LLM call MUST go through `LlmGateway` (`R-ARCH-03`, `R-CONC-04`).
- Keep type definitions and trait derives minimal and strictly necessary for production code: do **not** add test-only derives (such as `PartialEq`, `Eq`), speculative helper methods, or extra fields to `core` or `pipeline` types; tests MUST use `matches!`, field-level assertions, or `serde_json::to_value` comparisons instead of polluting production definitions.
- Rust source files MUST remain comment-free (`R-CODE-05`) and unit tests MUST live in `tests/<source>_tests.rs` (`R-CODE-02`).

---

## 3. Contracts

### 3.1 Knowledge Views and Validation (`extract_knowledge.rs`)

Create `crates/aise-core/src/pipeline/extract/extract_knowledge.rs`:

```rust
use crate::core::{
    ActivationRule, BoundedText, FactBody, FactChangeOp, FactId, KnowledgeChange, MemoryBody, MemoryId, RetrievalHint,
    RoleId, RoleMemoryChangeOp, RumorBody, RumorChangeOp, RumorId, StoryContext, TurnNumber,
};
use crate::pipeline::common::PipelineError;
use crate::pipeline::extract::ExtractConfig;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct FactView<'a> {
    pub id: &'a FactId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

#[derive(Serialize)]
pub(super) struct RumorView<'a> {
    pub id: &'a RumorId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

#[derive(Serialize)]
pub(super) struct ExtractRoleCatalogEntry<'a> {
    pub role_id: &'a RoleId,
    pub name: &'a BoundedText,
    pub aliases: &'a [BoundedText],
    pub retrieval_hint: &'a RetrievalHint,
}

#[derive(Serialize)]
pub(super) struct RoleMemoryView<'a> {
    pub id: &'a MemoryId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

pub(super) struct WorldKnowledgeExtraction {
    pub facts: Vec<FactChangeOp>,
    pub rumors: Vec<RumorChangeOp>,
    pub roles_for_memory: Vec<RoleId>,
}

pub(super) fn project_facts(story_ctx: &StoryContext) -> Vec<FactView<'_>>;
pub(super) fn project_rumors(story_ctx: &StoryContext) -> Vec<RumorView<'_>>;
pub(super) fn project_role_catalog(story_ctx: &StoryContext) -> Vec<ExtractRoleCatalogEntry<'_>>;
pub(super) fn project_role_memories<'a>(
    story_ctx: &'a StoryContext,
    role_id: &RoleId,
) -> Result<Vec<RoleMemoryView<'a>>, PipelineError>;

pub(super) fn allocate_fact_id(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<FactId, PipelineError>;

pub(super) fn allocate_rumor_id(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<RumorId, PipelineError>;

pub(super) fn allocate_memory_id(
    story_ctx: &StoryContext,
    role_id: &RoleId,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<MemoryId, PipelineError>;
```

Deterministic ID format for newly added entries on `next_turn = story_ctx.turn_number.increment()` (with 1-based `seq` within the Turn, incrementing collision suffix if an ID already exists in `story_ctx`):
- `FactId`: `fact_t{turn}_{seq}` (e.g. `fact_t1_1`)
- `RumorId`: `rumor_t{turn}_{seq}` (e.g. `rumor_t1_1`)
- `MemoryId`: `mem_t{turn}_{seq}` (e.g. `mem_t1_1`)

### 3.2 LLM Output Protocols (`extract_llm.rs`)

#### 3.2.1 `extract.extract_world_knowledge` JSON Protocol

```json
{
  "facts": [
    {
      "op": "add",
      "body": {
        "content": "The bronze seal was taken from the ancestral hall by Bai Suzhen.",
        "retrieval": "Location and holder of the bronze seal",
        "activation": {
          "match": {
            "keys": ["bronze seal", "ancestral hall"],
            "secondary_keys": [],
            "secondary_logic": "and_any",
            "case_sensitive": false,
            "match_whole_words": false,
            "scan_depth": null
          },
          "mode": { "enabled": true, "constant": false },
          "recursion": {
            "exclude_recursion": false,
            "prevent_recursion": false,
            "delay_until_recursion": null
          },
          "selection": {
            "order": 0,
            "probability": 100,
            "groups": [],
            "group_override": false,
            "group_weight": 100,
            "use_group_scoring": false
          }
        }
      }
    },
    {
      "op": "update",
      "id": "fact_seal_location",
      "body": {
        "content": "The bronze seal was originally kept in the ancestral hall, and has now been taken by Bai Suzhen.",
        "retrieval": "History and current holder of the bronze seal",
        "activation": {
          "match": { "keys": ["bronze seal", "ancestral hall"] }
        }
      }
    }
  ],
  "rumors": [
    {
      "op": "add",
      "body": {
        "content": "Townspeople believe a thief broke into the ancestral hall at night.",
        "retrieval": "Public rumor about the ancestral hall theft",
        "activation": {
          "match": { "keys": ["ancestral hall", "thief"] }
        }
      }
    },
    {
      "op": "update",
      "id": "rumor_hall_guarded",
      "body": {
        "content": "Townspeople now believe the ancestral hall guards failed to stop an intruder.",
        "retrieval": "Public consensus on the ancestral hall guards",
        "activation": {
          "match": { "keys": ["ancestral hall", "guards"] }
        }
      }
    },
    {
      "op": "remove",
      "id": "rumor_hall_untouched"
    }
  ],
  "roles_for_memory": ["bai_suzhen"]
}
```

DTO definitions in `extract_llm.rs`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldKnowledgeLlmDto {
    facts: Vec<FactProposalDto>,
    rumors: Vec<RumorProposalDto>,
    roles_for_memory: Vec<RoleId>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum FactProposalDto {
    Add { body: FactBody },
    Update { id: FactId, body: FactBody },
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum RumorProposalDto {
    Add { body: RumorBody },
    Update { id: RumorId, body: RumorBody },
    Remove { id: RumorId },
}
```

#### 3.2.2 `extract.extract_role_memory` JSON Protocol

```json
{
  "memories": [
    {
      "op": "add",
      "body": {
        "content": "I secretly took the bronze seal from the ancestral hall before dawn.",
        "retrieval": "Taking the bronze seal from the ancestral hall",
        "activation": {
          "match": { "keys": ["bronze seal", "ancestral hall"] }
        }
      }
    },
    {
      "op": "update",
      "id": "mem_seal_plan",
      "body": {
        "content": "I had planned to inspect the ancestral hall seal, and I have now taken it into my possession.",
        "retrieval": "Plan and acquisition of the ancestral hall seal",
        "activation": {
          "match": { "keys": ["bronze seal", "ancestral hall"] }
        }
      }
    }
  ]
}
```

DTO definitions in `extract_llm.rs`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleMemoryLlmDto {
    memories: Vec<RoleMemoryProposalDto>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum RoleMemoryProposalDto {
    Add { body: MemoryBody },
    Update { id: MemoryId, body: MemoryBody },
}
```

### 3.3 Prompt Contracts (`extract_prompt.rs`)

Register in `crates/aise-core/assets/prompts/index.toml`:

```toml
[[prompts]]
id = "extract.extract_world_knowledge"
csi = "csi/extract-extract-world-knowledge.md.j2"
rc = "rc/extract-extract-world-knowledge.md.j2"
fti = "fti/extract-extract-world-knowledge.md.j2"

[[prompts]]
id = "extract.extract_role_memory"
csi = "csi/extract-extract-role-memory.md.j2"
rc = "rc/extract-extract-role-memory.md.j2"
fti = "fti/extract-extract-role-memory.md.j2"
```

Prompt variables for `extract.extract_world_knowledge`:
- `story_summary`
- `story_opening`
- `recent_story`
- `current_proposal`
- `existing_facts`: JSON array of `FactView` (`id`, `content`, `retrieval`, `activation`).
- `existing_rumors`: JSON array of `RumorView` (`id`, `content`, `retrieval`, `activation`).
- `role_catalog`: JSON array of `ExtractRoleCatalogEntry` (`role_id`, `name`, `aliases`, `retrieval_hint`).
- `max_fact_changes`: `config.max_fact_changes`.
- `max_rumor_changes`: `config.max_rumor_changes`.
- `max_roles_for_memory`: `config.max_roles_for_memory`.

Prompt variables for `extract.extract_role_memory`:
- `story_summary`
- `story_opening`
- `recent_story`
- `current_proposal`
- `target_role_id`
- `target_name`
- `target_aliases`: JSON array of aliases.
- `target_backstory`
- `existing_memories`: JSON array of `RoleMemoryView` (`id`, `content`, `retrieval`, `activation`) for `target_role_id` only.
- `max_memory_changes_per_role`: `config.max_memory_changes_per_role`.

### 3.4 Orchestration Contract (`ExtractPipeline::execute`)

```rust
let (narrative_change, world_extraction) = tokio::try_join!(
    self.evaluate_narrative(&story_pack, story_ctx, input.proposal, control, observation),
    self.extract_world_knowledge(&story_pack, story_ctx, input.proposal, control, observation),
)?;
let role_memories = self
    .extract_role_memories(
        &story_pack,
        story_ctx,
        input.proposal,
        &world_extraction.roles_for_memory,
        control,
        observation,
    )
    .await?;
```

### 3.5 File / Directory Layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/
│   │   ├── extract-evaluate-narrative.md.j2
│   │   ├── extract-extract-role-memory.md.j2
│   │   └── extract-extract-world-knowledge.md.j2
│   ├── fti/
│   │   ├── extract-evaluate-narrative.md.j2
│   │   ├── extract-extract-role-memory.md.j2
│   │   └── extract-extract-world-knowledge.md.j2
│   ├── rc/
│   │   ├── extract-evaluate-narrative.md.j2
│   │   ├── extract-extract-role-memory.md.j2
│   │   └── extract-extract-world-knowledge.md.j2
│   └── index.toml
└── src/pipeline/extract/
    ├── config.rs
    ├── extract.rs
    ├── extract_knowledge.rs
    ├── extract_llm.rs
    ├── extract_narrative.rs
    ├── extract_prompt.rs
    ├── extract_trace.rs
    ├── mod.rs
    └── tests/
        ├── config_tests.rs
        ├── extract_knowledge_tests.rs
        ├── extract_llm_tests.rs
        ├── extract_narrative_tests.rs
        ├── extract_prompt_tests.rs
        └── extract_tests.rs
```

---

## 4. Behavior Rules

1. **R-P2-1 (Two-Stage Isolation)**: `extract.extract_world_knowledge` MUST receive full `story_ctx.facts`, full `story_ctx.rumors`, and the lightweight `role_catalog` (without any role's `memories`). `extract.extract_role_memory` for `role_id` MUST receive only `story_ctx.roles[&role_id].memories` and MUST NOT receive `story_ctx.facts` or any other role's `memories`.
2. **R-P2-2 (`Fact` Evolution Rules)**: `facts` proposals in `extract_world_knowledge` support only `Add` and `Update`. `Update` target `FactId` MUST exist in `story_ctx.facts`, and no `FactId` may appear more than once in `facts`. Total `facts` operations MUST NOT exceed `config.max_fact_changes`.
3. **R-P2-3 (`Rumor` Lifecycle Rules)**: `rumors` proposals in `extract_world_knowledge` support `Add`, `Update`, and `Remove`. `Update` and `Remove` target `RumorId` MUST exist in `story_ctx.rumors`, and no `RumorId` may appear more than once across `Update`/`Remove` in the same Turn. Total `rumors` operations MUST NOT exceed `config.max_rumor_changes`.
4. **R-P2-4 (`roles_for_memory` Validation and Zero-Waste Fast Path)**: Every `RoleId` in `roles_for_memory` MUST exist in `story_ctx.roles`, contain no duplicates, and have length `<= config.max_roles_for_memory`. When `roles_for_memory.is_empty()`, `extract_role_memories` MUST perform 0 LLM calls and return an empty `Vec<RoleMemoryChangeOp>`.
5. **R-P2-5 (Per-Role `Memory` Validation)**: For each `role_id` in `roles_for_memory`, `memories` proposals support only `Add` and `Update`. Every `Update` target `MemoryId` MUST exist in `story_ctx.roles[&role_id].memories`, no `MemoryId` may appear more than once, and total operations for that role MUST NOT exceed `config.max_memory_changes_per_role`.
6. **R-P2-6 (`ActivationRule` Validation)**: Deserialization of `FactBody`, `RumorBody`, and `MemoryBody` MUST enforce `ActivationKey::try_new` (non-empty after trim, `<= 128` bytes), `ActivationGroupKey::try_new` (valid lowercase ASCII/digit/separator format, `<= 128` bytes), `ActivationProbability::try_new` (`0..=100`), and `BoundedText` limits on `content` and `retrieval`. Further rule-level bounds (e.g. non-empty `match.keys` when `mode.constant == false`) MUST be validated before accepting any `Add` or `Update`.
7. **R-P2-7 (Perspective Boundary in `extract_role_memory` Prompt)**: The CSI/FTI templates for `extract.extract_role_memory` MUST explicitly instruct the LLM that only events the target role personally experienced, witnessed, heard, was explicitly told, or reasonably inferred from prior knowledge in `current_proposal` may enter `memories`; omniscient narration, other characters' inner thoughts, and off-scene events are forbidden, and character misunderstandings must be recorded from the character's subjective viewpoint.

### 4.1 Error Handling

`extract_knowledge` and `extract_llm` return `PipelineError::InvalidOutput { stage: PipelineStage::Extract, code, message }` with stable codes:
- `extract_world_invalid_json`: malformed JSON, unknown fields, or invalid `BoundedText` / `ActivationRule` in `extract_world_knowledge`.
- `extract_fact_changes_exceeded`: `facts.len() > config.max_fact_changes`.
- `extract_fact_unknown_id`: `FactProposalDto::Update` references a `FactId` not in `story_ctx.facts`.
- `extract_fact_duplicate_id`: the same `FactId` is updated more than once in a single Turn.
- `extract_rumor_changes_exceeded`: `rumors.len() > config.max_rumor_changes`.
- `extract_rumor_unknown_id`: `RumorProposalDto::Update` or `Remove` references a `RumorId` not in `story_ctx.rumors`.
- `extract_rumor_duplicate_id`: the same `RumorId` is targeted more than once in a single Turn.
- `extract_roles_for_memory_exceeded`: `roles_for_memory.len() > config.max_roles_for_memory`.
- `extract_role_unknown`: a `RoleId` in `roles_for_memory` is not in `story_ctx.roles`.
- `extract_role_duplicate`: a `RoleId` appears more than once in `roles_for_memory`.
- `extract_memory_invalid_json`: malformed JSON, unknown fields, or invalid `BoundedText` / `ActivationRule` in `extract_role_memory`.
- `extract_memory_changes_exceeded`: `memories.len() > config.max_memory_changes_per_role`.
- `extract_memory_unknown_id`: `RoleMemoryProposalDto::Update` references a `MemoryId` not in `story_ctx.roles[&role_id].memories`.
- `extract_memory_duplicate_id`: the same `MemoryId` is updated more than once for the same role in a single Turn.
- `extract_activation_rule_invalid`: `ActivationRule` has `mode.enabled == true`, `mode.constant == false`, and `match_rule.keys.is_empty()`.

### 4.2 Concurrency

- `evaluate_narrative` and `extract_world_knowledge` execute concurrently via `tokio::try_join!`.
- `extract_role_memories` executes across `roles_for_memory` in `roles_for_memory` order using ` futures::stream::iter(...).buffered(self.config.max_memory_concurrency.get()).try_collect().await`, preserving deterministic output ordering in `KnowledgeChange.memories`.
- Every LLM call routes through the shared `LlmGateway` (`R-CONC-04`).

### 4.3 Observability

- `extract_world_knowledge` creates a child `ObservationKind::Span` named `"extract_world_knowledge"` under the stage `&Observation` via `extract_trace::begin_extract_world_knowledge` / `finish_extract_world_knowledge`.
- Each role in `roles_for_memory` creates a child `ObservationKind::Span` named `"extract_role_memory"` under the stage `&Observation` via `extract_trace::begin_extract_role_memory` / `finish_extract_role_memory` with `role_id` recorded as a structured attribute (`R-OBS-02`, `R-OBS-04`).

---

## 5. Acceptance Criteria

- [ ] Prompts `extract.extract_world_knowledge` and `extract.extract_role_memory` are registered in `crates/aise-core/assets/prompts/index.toml` and their CSI/RC/FTI files exist.
- [ ] `extract_prompt_tests` verifies that `extract.extract_world_knowledge` includes `existing_facts`, `existing_rumors`, and `role_catalog` without leaking any role's `memories`, and `extract.extract_role_memory` includes only the target role's `existing_memories`.
- [ ] `extract_knowledge_tests` and `extract_llm_tests` cover every error code listed in §4.1, including `Fact` `Remove` rejection (via serde `deny_unknown_fields` / enum variants), `Memory` `Remove` rejection, and invalid `ActivationRule` rejection.
- [ ] `extract_tests` verifies concurrent execution of `evaluate_narrative` + `extract_world_knowledge`, 0 `extract_role_memory` calls when `roles_for_memory` is empty, and bounded-concurrent `extract_role_memory` calls when `roles_for_memory` is non-empty.
- [ ] End-to-end `runtime_tests` verifies that `ExtractOutput.knowledge_change` is committed via `CommitPipeline` into `StoryContext` and observed by the next Turn's `baseline`, `plan`, `retrieval`, and `think` stages.
- [ ] `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- Final numeric capacity and byte budget values (`ExtractConfig` limits, cumulative `StoryContext` entry caps, and prompt/output byte budgets) are marked `TBD` in the source design and will be finalized in a dedicated budget specification before implementation sign-off.
- Persistent SQLite `StoryStore` implementation of `KnowledgeChange` (when `story_store_sqlite` is introduced) will follow the same atomic validation and commit semantics as `StoryStoreMem`.

---

## 7. References

- Source design: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
- Phase 0: [StoryContext, KnowledgeChange, and Store Wiring](./2026-10-17-extract-pipeline-spec-phase-0-gemini.md)
- Phase 1: [Narrative Extract Loop](./2026-10-17-extract-pipeline-spec-phase-1-gemini.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
