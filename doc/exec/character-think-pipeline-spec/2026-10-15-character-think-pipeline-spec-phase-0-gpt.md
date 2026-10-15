# Character Think Role Catalog and Contracts — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-15
> **Status**: Proposed
> **Source Design**: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
> **Phase**: Phase 0

---

## 1. Goal

Establish bounded role assets, shared Character Think domain contracts, and a strictly validated Plan role-selection protocol.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add hard Story Pack limits for role count, aliases, catalog bytes, seed memories, and total serialized role bytes.
- Validate role assets before a story is frozen or retained.
- Add shared `CharacterThinkRequest`, `ResolvedRole`, and `CharacterDecision` domain types.
- Extend `PlanConfig` with authoritative role-selection and Thinking Focus limits.
- Build a complete, stable, bounded role catalog from `StoryPack.roles`.
- Add `roles_for_generation` and `character_think_requests` to strict Plan JSON output.
- Reject unknown, duplicate, oversized, and player-targeted role selections.
- Extend Plan prompt assets with the complete lightweight role catalog.

### 2.2 Non-Goals

- Does not resolve `RoleActor::Default` or `RoleActor::Card`.
- Does not execute Character Think calls.
- Does not pass role data or Character Decisions to Generate.
- Does not add role activation, presence, location, emotion, or scene-state extraction.
- Does not add runtime character memory or persist Character Decisions.
- Does not add a second Planner call.

### 2.3 Implementation Constraints

- Phases 0–3 form one hard-refactor change and MUST be reviewed and landed together.
- Do not keep fallback fields, compatibility aliases, permissive parsers, or dual prompt protocols.
- `Runtime::run_turn` remains the only Turn orchestrator.
- Pipeline stages MUST NOT import one another.
- Shared cross-stage domain values live in `core`; Plan-only catalog projections stay in `pipeline/plan`.
- `mod.rs` and `lib.rs` remain index-only.
- Tests live in `tests/<source>_tests.rs`.
- No code comments are permitted.
- Maintained crates MUST NOT depend on or modify `crates/aise` or `crates/aise-server`.

---

## 3. Contracts

### 3.1 Story Pack role limits

Add these authoritative constants to `crates/aise-core/src/core/story/story_pack.rs`:

```rust
pub const MAX_ROLES_PER_PACK: usize = 64;
pub const MAX_ALIASES_PER_ROLE: usize = 8;
pub const MAX_ROLE_CATALOG_BYTES: usize = 32 * 1024;
pub const MAX_SEED_MEMORIES_PER_ROLE: usize = 32;
pub const MAX_ROLE_ASSETS_BYTES: usize = 2 * 1024 * 1024;
```

Add:

```rust
impl StoryPack {
    pub fn validate_roles(&self) -> Result<(), StoryError>;
}
```

`validate_roles` computes `MAX_ROLE_CATALOG_BYTES` from the UTF-8 byte lengths of every role ID, role name, alias, and retrieval hint. It computes `MAX_ROLE_ASSETS_BYTES` from `serde_json::to_vec(&self.roles)`.

Extend `StoryError` with:

```rust
#[error("story pack role count {actual} exceeds maximum {maximum}")]
TooManyRoles { actual: usize, maximum: usize },

#[error("role {role_id} alias count {actual} exceeds maximum {maximum}")]
TooManyRoleAliases {
    role_id: RoleId,
    actual: usize,
    maximum: usize,
},

#[error("story pack role catalog size {actual} exceeds maximum {maximum}")]
RoleCatalogTooLarge { actual: usize, maximum: usize },

#[error("role {role_id} seed memory count {actual} exceeds maximum {maximum}")]
TooManySeedMemories {
    role_id: RoleId,
    actual: usize,
    maximum: usize,
},

#[error("story pack role assets size {actual} exceeds maximum {maximum}")]
RoleAssetsTooLarge { actual: usize, maximum: usize },

#[error("story pack role assets cannot be serialized: {message}")]
RoleAssetsSerialization { message: String },
```

`AiseFactory::freeze_story` MUST call:

```rust
spec.story_pack.validate_play()?;
spec.story_pack.validate_narrative()?;
spec.story_pack.validate_roles()?;
```

### 3.2 Shared domain types

Create `crates/aise-core/src/core/role/character_think.rs`:

```rust
use crate::core::{
    BoundedText, CharacterProfile, MemoryBody, MemoryId, RetrievalHint, RoleId,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkRequest {
    pub role_id: RoleId,
    pub thinking_focus: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedRole {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub aliases: Vec<BoundedText>,
    pub backstory: BoundedText,
    pub retrieval_hint: RetrievalHint,
    pub profile: CharacterProfile,
    pub seed_memories: BTreeMap<MemoryId, MemoryBody>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterDecision {
    pub role_id: RoleId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
```

Create `core/role/player_context.rs` with `PlayerRoleContext { role_id, name, aliases, backstory, profile }`. It contains no retrieval hint or memory. Re-export all four shared types through `core/role/mod.rs` and `core/mod.rs`.

### 3.3 Plan configuration

Extend `PlanConfig`:

```rust
const DEFAULT_MAX_ROLES_FOR_GENERATION: usize = 16;
const DEFAULT_MAX_CHARACTER_THINK_REQUESTS: usize = 8;
const DEFAULT_MAX_THINKING_FOCUS_BYTES: usize = 512;
const DEFAULT_MAX_PLAYER_ROLE_CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub max_retrieval_candidates_per_kind: usize,
    pub max_story_goal_bytes: usize,
    pub max_player_role_context_bytes: usize,
    pub max_roles_for_generation: usize,
    pub max_character_think_requests: usize,
    pub max_thinking_focus_bytes: usize,
}
```

`PlanConfig::default()` MUST preserve the existing `32` and `512` defaults and add `16384`, `16`, `8`, and `512` respectively.

### 3.4 Plan role catalog

Create `crates/aise-core/src/pipeline/plan/plan_roles.rs`:

```rust
use crate::core::{BoundedText, RetrievalHint, RoleId, StoryPack};

pub(super) struct RoleCatalogEntry<'a> {
    pub role_id: &'a RoleId,
    pub name: &'a BoundedText,
    pub aliases: &'a [BoundedText],
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) fn collect_role_catalog(
    story_pack: &StoryPack,
    player_role: &RoleId,
) -> Vec<RoleCatalogEntry<'_>>;

pub(super) fn contains_role(
    catalog: &[RoleCatalogEntry<'_>],
    role_id: &RoleId,
) -> bool;
```

`collect_role_catalog` iterates `StoryPack.roles` directly, excludes `player_role`, and preserves `BTreeMap<RoleId, _>` order. It MUST NOT truncate, rank, activate, or omit any other validated role. Plan deterministically resolves the player RoleBody plus effective Cast/Card profile into `PlayerRoleContext`.

### 3.5 Plan stage output

The final `PlanOutput` shape is:

```rust
#[derive(Serialize)]
pub struct PlanOutput {
    pub story_goal: BoundedText,
    pub player_role: PlayerRoleContext,
    pub facts_to_retrieve: Vec<FactId>,
    pub rumors_to_retrieve: Vec<RumorId>,
    pub roles_for_generation: Vec<RoleId>,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        !self.facts_to_retrieve.is_empty()
            || !self.rumors_to_retrieve.is_empty()
            || !self.roles_for_generation.is_empty()
            || !self.character_think_requests.is_empty()
    }

    pub fn requires_character_thinking(&self) -> bool {
        !self.character_think_requests.is_empty()
    }
}
```

### 3.6 Plan LLM protocol

Extend the private DTO in `plan_output.rs`:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterThinkRequestDto {
    role_id: RoleId,
    thinking_focus: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanLlmOutputDto {
    story_goal: String,
    facts_to_retrieve: Vec<FactId>,
    rumors_to_retrieve: Vec<RumorId>,
    roles_for_generation: Vec<RoleId>,
    character_think_requests: Vec<CharacterThinkRequestDto>,
}
```

Change the parser signature to:

```rust
pub(super) fn parse_plan_output(
    raw: &str,
    candidates: &PlanCandidates<'_>,
    role_catalog: &[RoleCatalogEntry<'_>],
    player_role: PlayerRoleContext,
    config: &PlanConfig,
) -> Result<PlanOutput, PipelineError>;
```

The exact JSON protocol is:

```json
{
  "story_goal": "one concise sentence within max_story_goal_bytes",
  "facts_to_retrieve": ["fact-id"],
  "rumors_to_retrieve": ["rumor-id"],
  "roles_for_generation": ["role-id"],
  "character_think_requests": [
    {
      "role_id": "role-id",
      "thinking_focus": "non-empty string"
    }
  ]
}
```

Use these additional stable `PipelineError::InvalidOutput.code` values:

```text
plan_too_many_generation_roles
plan_duplicate_generation_role_id
plan_unknown_generation_role_id
plan_player_generation_role_forbidden
plan_too_many_character_think_requests
plan_duplicate_character_think_role_id
plan_unknown_character_think_role_id
plan_player_character_think_forbidden
plan_thinking_focus_empty
plan_thinking_focus_too_long
```

### 3.7 Plan prompt projection

Add `player_role` and `role_catalog` to `plan_prompt::process_plan` and add the Prompt variables:

```text
player_role
role_catalog
```

Player Character renders RoleId, name, aliases, appearance, personality, and backstory. It MUST omit speaking style, dialogue examples, retrieval hint, and memory, and fit `max_player_role_context_bytes`.

Each role renders in stable order as:

```text
- role_id: "<JSON-escaped RoleId>"
  name: "<JSON-escaped name>"
  aliases: ["<JSON-escaped alias>"]
  retrieval_hint: "<JSON-escaped retrieval hint>"
```

The Plan RC order is:

```text
Story Summary or Story Opening
Recent Story
Player Character
Known Facts
Rumors
Retrievable Facts
Retrievable Rumors
AI Role Catalog
Narrative Direction
Story Constraints
Pending Player Input
```

The Plan CSI/FTI MUST state:

- `story_goal` contains only one concise immediate narrative transition and stays within the configured UTF-8 byte limit.
- Player Character is always passed directly to Generate and is never selectable.
- The AI catalog contains every selectable non-player role.
- Role relevance and presence are inferred from story text.
- `roles_for_generation` selects roles whose complete data Generate needs.
- `character_think_requests` selects only non-player roles needing an independent current-Turn decision.
- Every returned RoleId must come from AI Role Catalog.
- A Think target need not be repeated in `roles_for_generation`.
- Empty arrays are valid.
- No extra fields or prose are allowed.

### 3.8 File layout

```text
crates/aise-core/src/
  core/
    role/
      character_think.rs
      mod.rs
    story/
      error.rs
      story_pack.rs
  pipeline/
    plan/
      config.rs
      mod.rs
      plan.rs
      plan_output.rs
      plan_prompt.rs
      plan_roles.rs
      plan_trace.rs
      tests/
        config_tests.rs
        plan_output_tests.rs
        plan_prompt_tests.rs
        plan_roles_tests.rs
```

---

## 4. Behavior Rules

1. **R-CT-P0-1**: A Story Pack exceeding any §3.1 limit fails `AiseFactory::create_story` before assets are retained.
2. **R-CT-P0-2**: Catalog byte accounting includes every role ID, name, alias, and retrieval hint exactly once.
3. **R-CT-P0-3**: Plan receives every role from a valid Story Pack in ascending `RoleId` order.
4. **R-CT-P0-4**: Plan MUST NOT run role activation or truncate a valid role catalog.
5. **R-CT-P0-5**: Plan output arrays preserve LLM order after validation.
6. **R-CT-P0-6**: `roles_for_generation` is unique and contains only catalog RoleIds.
7. **R-CT-P0-7**: Character Think requests are unique by RoleId, contain only catalog RoleIds, and never target `StoryContext.player`.
8. **R-CT-P0-8**: Thinking Focus is trimmed, non-empty, and bounded by `max_thinking_focus_bytes` before `BoundedText` construction.
9. **R-CT-P0-9**: The model never returns role profile, backstory, memories, decision, or suggested utterance during Plan.
10. **R-CT-P0-10**: Malformed or invalid role selection fails the Plan stage; no invalid item is silently dropped.

### 4.1 Error Handling

- Story asset-limit failures use the exact `StoryError` variants in §3.1.
- LLM schema and role-selection failures use `PipelineError::InvalidOutput` with `stage = PipelineStage::Plan`.
- No `unwrap`, `expect`, panic, fallback parser, unknown-field tolerance, or Markdown-fence stripping is allowed for external data.

### 4.2 Concurrency

- This phase introduces no new task, queue, lock, or LLM call.
- Story Pack validation and role catalog collection are synchronous and bounded by §3.1.
- Plan still performs exactly one LLM call through the injected `LlmGateway`.

### 4.3 Observability

Extend `PlanTraceMetrics` with:

```rust
pub role_catalog_entries: usize,
pub role_catalog_bytes: usize,
pub generation_roles_selected: usize,
pub character_think_requests: usize,
```

Emit:

```text
aise.plan.roles.catalog_entries
aise.plan.roles.catalog_bytes
aise.plan.roles.generation_selected
aise.plan.roles.think_requests
```

Metrics MUST contain counts and byte sizes only. They MUST NOT contain names, aliases, backstories, memories, Thinking Focus, or Character Decisions.

---

## 5. Acceptance Criteria

- [ ] `StoryPack::validate_roles` enforces all five constants in §3.1.
- [ ] `AiseFactory::freeze_story` invokes `validate_roles` before persistence.
- [ ] Boundary tests cover each accepted limit and each value one over its limit.
- [ ] `CharacterThinkRequest`, `ResolvedRole`, and `CharacterDecision` match §3.2 and are re-exported from `core`.
- [ ] `PlanConfig::default()` returns `16`, `8`, and `512` for the new fields.
- [ ] `collect_role_catalog` returns every role in stable RoleId order without activation or truncation.
- [ ] Plan prompt tests prove the catalog contains ID, name, aliases, and retrieval hint but no backstory, profile, or memory.
- [ ] Plan output tests cover all nine stable error codes in §3.6.
- [ ] Plan rejects the player role as a Think target while allowing it in `roles_for_generation`.
- [ ] `requires_retrieval` and `requires_character_thinking` match §3.5.
- [ ] Plan observations expose all four metadata fields in §4.3.
- [ ] `rg "RoleCatalogEntry" crates/aise-core/src` shows matches only under `pipeline/plan`.

---

## 6. Out of Scope / Future Work

- Role resolution and Runtime stable union are specified in [Phase 1](./2026-10-15-character-think-pipeline-spec-phase-1-gpt.md).
- Character Think execution is specified in [Phase 2](./2026-10-15-character-think-pipeline-spec-phase-2-gpt.md).
- Generate integration and final verification are specified in [Phase 3](./2026-10-15-character-think-pipeline-spec-phase-3-gpt.md).

---

## 7. References

- Source design: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Architecture guardrails: `doc/agents/guardrails/architecture-refactor.md`
- Code organization: `doc/agents/guardrails/code-organization.md`
