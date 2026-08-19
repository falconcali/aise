# Player Contribution Realization — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-08-19
> **Status**: Proposed
> **Source Design**: [Player Contribution Realization — Design](../design/2026-08-19-player-contribution-realization-design-gpt.md)
> **Phase**: N-A

---

## 1. Goal

Replace the ambiguous `Player Input` contract with one end-to-end `Player Contribution` contract that forces every explicit Player Character utterance, attempted action, and private thought into the next continuous story segment before or while the story renders its causal response.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Rename every active Rust, prompt-variable, JSON, trace, configuration, persistence, DOM, JavaScript, error, and test identifier from `player_input` / `PlayerInput` / `Player Input` to the canonical `player_contribution` / `PlayerContribution` / `Pending Player Contribution` form defined in §3.1.
- Update the Runtime Context and CSI/FTI contracts for Writer Planner, Character Think, Story Generator, and Story Repairer so all four stages use the same pending-contribution lifecycle and stage-specific authority rules.
- Make Writer Planner include both contribution realization and its immediate causal response in `story_goal`, rather than planning only the aftermath.
- Make Character Think use only externally perceptible contribution components and never use private Player Character thoughts or requested external outcomes as Target Character knowledge.
- Make Story Generator visibly realize every explicit speech/action/thought component in prose; elaboration may support but must not replace supplied material.
- Make Story Repairer preserve or restore the same on-page realization contract in its complete replacement segment.
- Rename the Turn request, Turn execution accessors, retrieval signal origin, prompt projections, committed Turn metadata, history views, trace payload, and story-history byte-limit configuration.
- Add one forward SQLite migration that renames the live `story_turns` column without changing stored values.
- Change the bundled web client to submit `player_contribution` and render only committed opening/`story_text` prose in the story panel, with no separate `你：...` chat line.
- Update unit, integration, migration, prompt-contract, API serialization, trace, and full-workspace checks listed in §5.

### 2.2 Non-Goals

- Does not introduce a classifier, parser, enum, or additional LLM call that splits free-form contribution text into speech/action/thought fields.
- Does not add support for player-authored commands that require the world or another character to produce a requested outcome.
- Does not guarantee that any attempted Player Character action succeeds.
- Does not add a semantic LLM validator or a new Validation Pipeline stage for contribution realization.
- Does not expose raw `player_contribution` to Story State Extractor, deterministic validators, Narrative resolution, or future summary stages; those stages continue to consume candidate story/state artifacts.
- Does not change Writer Planner output JSON, Character Decision output JSON, Story Generator prose output, Story Repairer prose output, or State Extractor output schemas.
- Does not rewrite migration files `0001` through `0021` or archival design/spec/review documents that record the historical `player_input` name.
- Does not retain the old HTTP field, prompt slot, config field, trace field, database column, DOM id, or Rust symbol as a compatibility alias.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, `serde(alias)`, dual JSON fields, dual prompt variables, dual database columns, or dual-write logic.
- Old active types, variants, fields, functions, constants, local variables, error codes, tests, and client identifiers superseded by this spec MUST be renamed or deleted in the same change.
- No mid-state in which `player_input` and `player_contribution` coexist in active code or active prompt assets is allowed.
- Preserve the existing 4,096-character request bound, normalization behavior, request-digest bytes, token budgets, trace-content policy, LLM routing, and Turn ownership/lifetime.
- Historical migrations are immutable. The single permitted active occurrence of the legacy database identifier is the source column named by migration `0022_player_contribution.sql`.
- Follow `R-REFACTOR-01/02`, `R-CODE-01/02/03/05/06/07`, `R-OBS-01/02/04`, and `R-AISE-01/02/03/07` from `AGENTS.md`.
- No new dependency is permitted.

---

## 3. Contracts

### 3.1 Canonical Terminology and Lifecycle

| Boundary | Required name | Meaning |
|---|---|---|
| Product/domain concept | `Player Contribution` | The player's latest Player Character source material |
| Pre-commit prompt heading | `Pending Player Contribution` | The contribution has not yet appeared in committed Story Continuity |
| Rust field/function/local | `player_contribution` | The single Turn-owned normalized string |
| Rust enum variant | `PlayerContribution` | Origin of retrieval signals derived from the contribution |
| Prompt variable | `player_contribution` | Runtime data injected into the four active RC templates |
| HTTP/history JSON | `player_contribution` | Breaking replacement for `player_input` |
| Trace/config/database | `player_contribution` | Breaking replacement for `player_input` |
| DOM / JavaScript | `player-contribution` / `playerContribution` | Bundled client naming |

The lifecycle is exact:

1. Before commit, `player_contribution` is not part of `StoryContinuity` and has not happened in story prose.
2. Writer-side stages may read the entire contribution as untrusted Runtime Context data.
3. Character Think may use only components the Target Character can perceive or causally infer when they occur.
4. Story Generator realizes supported components in candidate `story_text` and resolves consequences causally.
5. After commit, `StoryTurn.player_contribution` is immutable Turn metadata; only committed `story_text` enters `StoryContinuity` and the continuous-story display.

### 3.2 Rust Contracts

`crates/aise/src/turn/turn_contract.rs` MUST expose the following final names and behavior:

```rust
pub const MAX_PLAYER_CONTRIBUTION_CHARS: usize = 4096;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TurnRequestError {
    EmptyIdempotencyKey,
    IdempotencyKeyTooLong { actual: usize, maximum: usize },
    EmptyPlayerContribution,
    PlayerContributionTooLong { actual: usize, maximum: usize },
}

#[derive(Debug, Clone)]
pub struct TurnRequest {
    player_contribution: String,
    request_digest: RequestDigest,
}

impl TurnRequest {
    pub fn try_new(player_contribution: String) -> Result<Self, TurnRequestError>;
    pub fn player_contribution(&self) -> &str;
    pub fn request_digest(&self) -> &RequestDigest;
}

#[derive(Debug, Clone)]
pub struct ExecuteTurnSpec {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_contribution: String,
    pub cancellation: TurnCancellation,
}
```

`TurnRequest::try_new` MUST trim the supplied string exactly once, validate the trimmed character count against `MAX_PLAYER_CONTRIBUTION_CHARS`, and compute `RequestDigest` from the same normalized UTF-8 bytes used before this refactor. Rename the private digest constructor to:

```rust
fn from_canonical_contribution(contribution: &str) -> RequestDigest;
```

The exact request error messages are:

```text
player contribution must not be empty
player contribution is {actual} chars, maximum {maximum}
```

`crates/aise/src/turn/turn_context.rs` MUST expose only:

```rust
pub fn player_contribution(&self) -> &str;
```

The following final domain, persistence, trace, and config shapes are required:

```rust
pub struct StoryTurn {
    pub number: TurnNumber,
    pub sequence: StorySequence,
    pub player_contribution: String,
    pub story_text: String,
    pub created_at: i64,
}

pub struct StoryTurnView {
    pub turn_number: TurnNumber,
    pub sequence: StorySequence,
    pub player_contribution: String,
    pub story_text: String,
    pub created_at: i64,
}

pub struct TurnData {
    pub story_id: String,
    pub turn_number: Option<TurnNumber>,
    pub player_contribution: String,
    pub status: String,
    pub error: Option<String>,
}

pub struct StoryHistoryConfig {
    pub default_page_size: usize,
    pub max_page_size: usize,
    pub max_player_contribution_bytes: usize,
    pub max_story_text_bytes: usize,
}

#[serde(rename_all = "snake_case")]
pub enum RetrievalSignalOrigin {
    PlayerContribution,
    RoleState,
    Narrative,
    RecentStory,
    Summary,
}
```

The serialized retrieval-origin value MUST be `player_contribution`.

### 3.3 Prompt Projection Contracts

The active prompt projection interfaces MUST use these names:

```rust
pub struct StoryGeneratorPromptContext {
    pub story_profile: StoryProfilePromptView,
    pub instance_settings: StoryGeneratorInstanceSettingsPromptView,
    pub story_continuity: StoryContinuityPromptView,
    pub player_role: StoryGeneratorRolePromptView,
    pub ai_roles: Vec<StoryGeneratorRolePromptView>,
    pub relevant_knowledge: WorldKnowledgePromptView,
    pub story_goal: BoundedText,
    pub narrative_direction: NarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_decisions: Vec<StoryGeneratorCharacterDecisionPromptView>,
    pub player_contribution: BoundedText,
}

pub struct CharacterThinkPromptContext {
    pub target_role: CharacterThinkRolePromptView,
    pub current_role_state: CharacterThinkStatePromptView,
    pub story_continuity: CharacterThinkStoryContinuityPromptView,
    pub narrative_character_impulses: Vec<CharacterThinkImpulsePromptView>,
    pub thinking_focus: BoundedText,
    pub player_contribution: BoundedText,
}

impl WriterPlannerPromptContextProjector {
    pub fn project(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        player_contribution: &BoundedText,
        max_input_tokens: u64,
    ) -> Result<WriterPlannerPromptProjection, WriterPlannerProjectionError>;
}
```

`StoryGeneratorProjectionError::InvalidPlayerInput` becomes:

```rust
#[error("story generator player contribution is invalid")]
InvalidPlayerContribution,
```

`StoryGenerator` maps that variant to the error code `invalid_player_contribution`. All `BoundedText` field labels and limit names use `player_contribution`.

Every active `RuntimePromptVars` map and every relevant `slots.yaml` entry MUST contain exactly the key:

```text
player_contribution
```

No active prompt map or slot registry may contain `player_input`. Story Repairer continues to reuse `StoryGeneratorPromptContext` through `StoryRepairerPromptContext.generation`; it MUST NOT create a second copy or a second variable name.

### 3.4 HTTP, History, Trace, and Client Contracts

The Turn POST body is a breaking contract:

```json
{
  "player_contribution": "你是谁",
  "include_trace": true
}
```

The server DTO is:

```rust
#[derive(Debug, Deserialize)]
pub struct TurnRequest {
    pub player_contribution: String,
    #[serde(default)]
    pub include_trace: bool,
}
```

`player_input` MUST NOT be accepted through a `serde` alias. The bundled client submits only `player_contribution`.

Each committed history Turn serializes as:

```json
{
  "turn_number": 1,
  "sequence": 2,
  "player_contribution": "你是谁",
  "story_text": "你隔着门问：……",
  "created_at": 0
}
```

Trace `SpanPayload::Turn` serializes `player_contribution` under the existing content policy and `MAX_LLM_CONTENT_CHARS` truncation. No additional raw-content field is added.

The bundled client MUST:

- rename DOM id `player-input` to `player-contribution`;
- rename JavaScript binding `playerInput` to `playerContribution` and local `input` to `contribution` within Turn submission;
- use placeholder `输入角色要说、做或想的内容…`;
- remove the optimistic `storyEl` append of `你：${contribution}`;
- render each committed history Turn by appending only `turn.story_text`;
- keep `turn.player_contribution` available in the history payload but not render it as a separate story line;
- submit `{ player_contribution: contribution, include_trace: traceEnabled }`.

### 3.5 SQLite Migration Contract

Add exactly one migration after `0021_turn_runtime_contract_alignment.sql`:

```text
crates/aise/assets/persistence/mig/0022_player_contribution.sql
```

Its schema operation is:

```sql
ALTER TABLE story_turns RENAME COLUMN player_input TO player_contribution;
```

All active SQL in `sqlite_store.rs` and `sqlite_story_history_reader.rs` MUST use `player_contribution`. The migration MUST preserve:

- all existing contribution text bytes;
- `story_id`, `turn_number`, `sequence`, `story_text`, and `created_at`;
- `idempotency_key`, `request_digest`, revision fields, status, and `result_json`;
- row count and uniqueness constraints.

Do not edit migration `0001`, `0002`, `0011`, or `0021`; their legacy column references remain historical migration source.

### 3.6 Runtime Context Heading Contract

The final required RC section for Writer Planner, Character Think, and Story Generator is:

```markdown
## Pending Player Contribution

{{ player_contribution }}
```

The nested section in Story Repairer is:

```markdown
### Pending Player Contribution

{{ player_contribution }}
```

Do not place an instruction sentence inside RC. The lifecycle authority belongs in CSI/FTI; the contribution remains untrusted data.

### 3.7 Writer Planner Prompt Contract

Keep the existing Writer Planner structure and `10 MUST / 3 SHOULD / 5 NEVER` CSI count. Replace all old terminology and require these exact semantics:

```text
MUST: Base the plan on the committed story state, the character and knowledge context already provided, the Pending Player Contribution, and the applicable narrative guidance and constraints.

MUST: Treat Pending Player Contribution as not-yet-narrated material for the next story segment. Preserve every explicitly supplied Player Character utterance, attempted action, and private thought when defining the transition, and do not assume an attempted outcome succeeds or invent additional player behavior.

SHOULD: Use Narrative Plan as direction while adapting the immediate transition causally to Story Continuity, Relevant Knowledge, and Pending Player Contribution.
```

Keep the existing `5 MUST / 3 NEVER` FTI count and use:

```text
MUST: Set `story_goal` to the immediate narrative transition, including both the in-story realization of Pending Player Contribution and the immediate causal response or progress that follows it.

NEVER: Treat Pending Player Contribution as already present in Story Continuity, skip its in-story realization, guarantee an attempted outcome, or invent additional player behavior.
```

For the regression input `“你是谁”`, a valid `story_goal` must cover the Player Character asking the question and the immediate response. A goal that says only “respond to the Player Character's question” is invalid because it presupposes an off-page event.

### 3.8 Character Think Prompt Contract

Keep `10 MUST / 3 SHOULD / 5 NEVER` in CSI and `5 MUST / 3 NEVER` in FTI. Replace the existing contribution rule with:

```text
MUST: Treat Pending Player Contribution as not-yet-narrated Player Character source material. Use an explicitly supplied utterance or attempted action only when the Target Character could perceive it as it occurs; never use a private Player Character thought or desired external outcome as character knowledge, and never assume an attempted outcome succeeds.
```

The FTI reminder is:

```text
MUST: Use only externally perceptible parts of Pending Player Contribution as immediate decision context, preserve Player Character and other-character autonomy, and do not treat attempts as guaranteed outcomes or private Player Character thoughts as Target Character knowledge.
```

The matching CSI prohibition is:

```text
NEVER: Treat Pending Player Contribution, the Target Character's decision, or a requested external outcome as guaranteed success, committed world state, or a future event; never expose a private Player Character thought to the Target Character without an independent authorized basis.
```

All other Character Think epistemic, output, and autonomy rules remain unchanged except for canonical terminology.

### 3.9 Story Generator Prompt Contract

Replace the Objective with:

```text
Generate exactly one new story segment that continues from the committed story state, realizes the Pending Player Contribution inside the prose, and advances the scene through causal responses while pursuing the Immediate Story Goal. Return only the generated prose; state extraction happens in a separate stage.
```

CSI MUST contain exactly 10 rules. Replace the current single realization rule with these two rules, producing `10 MUST / 3 SHOULD / 5 NEVER`:

```text
MUST: Treat Pending Player Contribution as not-yet-narrated source material for this segment, not as an out-of-story message to answer or an event already present in Story Continuity.

MUST: Realize every explicitly supplied Player Character utterance, attempted action, and private thought inside the prose before or while showing the world's response. You may adapt wording, order, point of view, tense, staging, reactions, and non-consequential detail for natural continuity, but preserve each supplied component's essential meaning and use elaboration to support rather than replace it. Resolve attempted outcomes through story causality, treat private thoughts only as subjective state, and treat requested external outcomes as non-authoritative.
```

Replace `Player Input intent` elsewhere with `Pending Player Contribution's supported intent`. Replace the existing Player Input NEVER rule with:

```text
NEVER: Omit, merely imply, or jump past an explicitly supplied Player Character utterance, attempted action, or private thought; replace it with invented player behavior; change its essential intent; make an unprovided consequential choice for the player; or treat an attempted or requested external outcome as guaranteed.
```

Keep FTI at `5 MUST / 3 NEVER`. Replace its realization reminder with:

```text
MUST: Put every explicitly supplied Player Character utterance, attempted action, and private thought on the page before or while showing the world's response; preserve each component's essential meaning, use elaboration to support rather than replace it, leave unprovided consequential choices to the player, and resolve attempted outcomes through story causality.
```

Replace the FTI input prohibition with canonical terminology and preserve its existing continuity, constraint, and AI-character-agency precedence.

### 3.10 Story Repairer Prompt Contract

Story Repairer CSI MUST become `10 MUST / 3 SHOULD / 5 NEVER`. Replace its current one-rule Player Input treatment with these three rules:

```text
MUST: Treat Pending Player Contribution as the same not-yet-committed source material used for the original generation, not as an out-of-story request or an event independently established outside the segment.

MUST: Preserve or restore the on-page realization of every explicitly supplied Player Character utterance, attempted action, and private thought; if Previous Story Text skipped a supplied component, include it in the complete replacement segment while keeping the repair minimal and coherent.

MUST: Preserve each supplied component's essential meaning and consequential choices, use elaboration only to support it, resolve attempts causally, keep private thoughts subjective, and never guarantee a requested external outcome.
```

Replace its matching NEVER rule with:

```text
NEVER: Violate committed continuity, hard constraints, Pending Player Contribution realization or intent, character agency, private state, or knowledge boundaries merely to clear validation.
```

Keep Story Repairer FTI at `5 MUST / 3 NEVER`. Its authoritative-context reminder becomes:

```text
MUST: Preserve authoritative generation context: committed continuity and hard constraints, every supported component and the autonomy boundary of Pending Player Contribution, every provided AI Character Decision and character knowledge boundary, and the existing Immediate Story Goal.
```

### 3.11 Pipeline Coverage Contract

| Stage / boundary | Required change | Required invariant |
|---|---|---|
| HTTP Turn DTO | `player_contribution` | Old JSON field has no alias |
| `TurnRequest` validation | Rename constant, variants, field, accessor, errors | Normalized bytes and 4,096-char bound unchanged |
| `TurnInitializer` | Use `ctx.player_contribution()` and renamed error text | Empty contribution still fails before LLM |
| `BaselineContextBuilder` | Rename parameter/local/accessor | Contribution is not copied into Baseline/Snapshot |
| `RetrievalSignalBuilder` | Rename parameter and origin variant | Raw text may drive writer retrieval, not committed facts |
| Writer Planner | Rename bounded field/slot and apply §3.7 | Goal includes realization plus causal transition |
| Context Retrieval | Consume renamed `PlayerContribution` signals | No new audience or permission path |
| Character Think | Rename field/slot and apply §3.8 | Private Player Character thought remains inaccessible |
| Story Generator | Rename field/error/slot and apply §3.9 | Explicit supported components appear in candidate prose |
| Story State Extractor | No contribution field added | Extract only from candidate story and existing state context |
| Deterministic Validation | No semantic validator added | Continue validating candidate story/state artifacts |
| Story Repairer | Reuse renamed generation context and apply §3.10 | Replacement prose satisfies realization contract |
| Turn Committer | Persist renamed Turn metadata | Story continuity receives only `story_text` |
| SQLite store/history | Rename active SQL/config/view fields; add `0022` | Upgrade preserves existing rows |
| Trace | Rename root payload field | Existing content policy and truncation unchanged |
| Story history API | Serialize `player_contribution` | Breaking field rename, no dual output |
| Web client | Rename request/DOM symbols and remove chat echo | Story panel displays opening plus `story_text` only |
| Debug shortened runtime | Use the same request/planner/generator contracts | No debug-only prompt or naming branch |

### 3.12 File Change Inventory

The implementation MUST inspect and update every path in this table. A listed “verify only” path must receive a test or assertion if necessary to prove the contract.

| Area | Files |
|---|---|
| Turn/domain | `crates/aise/src/turn/turn_contract.rs`, `crates/aise/src/turn/turn_context.rs`, `crates/aise/src/turn/turn_trace.rs`, `crates/aise/src/runtime/initializer.rs`, `crates/aise/src/engine.rs`, `crates/aise/src/domain/narrative.rs`, `crates/aise/src/domain/turn/retrieval.rs` |
| Context/retrieval | `crates/aise/src/context/baseline_ctx_builder.rs`, `crates/aise/src/context/retrieval_signal_builder.rs` |
| Planning | `crates/aise/src/planning/writer_planner.rs`, `crates/aise/src/planning/writer_planner_prompt.rs`, `crates/aise/src/planning/tests/writer_planner_prompt_tests.rs` |
| Character | `crates/aise/src/character/character_think_prompt.rs`, `crates/aise/src/character/tests/character_think_prompt_tests.rs` |
| Story | `crates/aise/src/story/story_generator_prompt.rs`, `crates/aise/src/story/story_generator.rs`, `crates/aise/src/story/tests/story_generator_prompt_tests.rs`, `crates/aise/src/story/tests/story_repairer_prompt_tests.rs`; verify `crates/aise/src/story/story_repairer_prompt.rs` still reuses one generation context |
| Prompt CSI | `crates/aise/assets/prompts/context-v2/csi/writer-planner.md.j2`, `crates/aise/assets/prompts/context-v2/csi/character-think.md.j2`, `crates/aise/assets/prompts/context-v2/csi/story-generator.md.j2`, `crates/aise/assets/prompts/context-v2/csi/story-repairer.md.j2` |
| Prompt RC | `crates/aise/assets/prompts/context-v2/rc/writer-planner.md.j2`, `crates/aise/assets/prompts/context-v2/rc/character-think.md.j2`, `crates/aise/assets/prompts/context-v2/rc/story-generator.md.j2`, `crates/aise/assets/prompts/context-v2/rc/story-repairer.md.j2` |
| Prompt FTI | `crates/aise/assets/prompts/context-v2/fti/writer-planner.md.j2`, `crates/aise/assets/prompts/context-v2/fti/character-think.md.j2`, `crates/aise/assets/prompts/context-v2/fti/story-generator.md.j2`, `crates/aise/assets/prompts/context-v2/fti/story-repairer.md.j2` |
| Prompt slots/tests | `crates/aise/assets/prompts/context-v2/slots.yaml`, `crates/aise/src/prompt/tests/trusted_prompt_source_tests.rs`, `crates/aise/tests/prompt_context_contract_tests.rs` |
| Persistence | `crates/aise/src/persistence/turn_committer.rs`, `crates/aise/src/persistence/sqlite_store.rs`, `crates/aise/src/persistence/sqlite_story_history_reader.rs`, `crates/aise/src/persistence/story_history_read_port.rs`, new `crates/aise/assets/persistence/mig/0022_player_contribution.sql` |
| Server/client | `crates/aise-server/src/api/dto.rs`, `crates/aise-server/src/api/turn.rs`, `crates/aise-server/assets/app.js`, `crates/aise-server/assets/index.html`, `crates/aise-server/assets/style.css`; verify history serialization through `crates/aise-server/src/api/story.rs` |
| Cross-cutting tests | `crates/aise/tests/domain_core_dependency_tests.rs`, `crates/aise/tests/persistence_tests.rs`, `crates/aise/tests/story_pack_runtime_tests.rs`, `crates/aise/tests/turn_trace_tests.rs`, new `crates/aise/tests/player_contribution_migration_tests.rs`, `crates/aise-server/tests/story_api_tests.rs` |

---

## 4. Behavior Rules

1. **PCR-1 — One owner**: The normalized `player_contribution` exists once in `TurnRequest` and is accessed through `TurnExecutionContext`; Baseline, Snapshot, Writer Plan, and Retrieved Context MUST NOT store independent text copies.
2. **PCR-2 — Pending boundary**: Before commit, the contribution is not part of `StoryContinuity`, has not happened off-page, and is not an out-of-story message for the model to answer.
3. **PCR-3 — Speech**: Explicit Player Character speech MUST be rendered as direct or indirect speech with its essential communicative intent intact. If speech is causally impossible, render the attempt and obstacle instead of omitting it.
4. **PCR-4 — Action**: Explicit Player Character action establishes the action or attempt, not automatic success, effect, another entity's response, or a changed world state.
5. **PCR-5 — Thought**: Explicit Player Character thought establishes only subjective private mental content; it MUST NOT establish objective truth or become AI-character knowledge without an independent authorized basis.
6. **PCR-6 — Mixed contribution**: When speech, action, and thought coexist, Story Generator may reorder them for causal prose but MUST visibly realize every explicit component exactly once in semantic effect.
7. **PCR-7 — Elaboration**: Point-of-view conversion, tense conversion, gestures, sensory details, connective actions, local reactions, and light paraphrase are permitted only when they preserve the supplied meaning and do not replace a supplied component or add a consequential Player Character choice.
8. **PCR-8 — Desired outcome**: A first-person hope may be rendered as subjective thought. A request that the world or another character produce an outcome is non-authoritative and MUST NOT be treated as established, guaranteed, or Player Character behavior solely because it appears in the contribution.
9. **PCR-9 — Planner**: Writer Planner MUST treat the contribution as pending and define a goal that covers both its in-story realization and the immediate causal transition.
10. **PCR-10 — Character Think**: Character Think MAY use impending externally perceptible speech/action as immediate decision context, but MUST ignore private Player Character thought and unsupported external outcomes.
11. **PCR-11 — Generator**: Story Generator MUST not jump directly from committed continuity to an NPC/world response while leaving the explicit Player Character contribution off-page.
12. **PCR-12 — Repairer**: Every replacement segment returned by Story Repairer MUST satisfy PCR-3 through PCR-8 even when the original candidate omitted a component.
13. **PCR-13 — Downstream authority**: State extraction, validation, narrative resolution, summary, and future continuity construction derive facts from final story/state artifacts, never directly from raw contribution text.
14. **PCR-14 — Continuous display**: The player-facing story is the concatenation of opening and committed `story_text` segments; raw contribution metadata is not rendered as a separate chat line.
15. **PCR-15 — Hard rename**: Active code/assets expose no old name, alias, fallback, or dual contract. Historical migrations and archival docs are the only exclusions.
16. **PCR-16 — Stable digest and bounds**: Renaming MUST NOT change normalization, 4,096-character validation, digest computation, token accounting, or trace truncation.
17. **PCR-17 — Data boundary**: Contribution text remains RC data and MUST NOT enter CSI/FTI through interpolation or gain instruction authority.
18. **PCR-18 — No new call path**: No pipeline, LLM call, queue, lock, or concurrency path is added; all existing LLM calls continue through `LlmGateway` and the shared limiter.

### 4.1 Error Handling

- Empty input returns `TurnRequestError::EmptyPlayerContribution` with `player contribution must not be empty`.
- Input over 4,096 characters returns `TurnRequestError::PlayerContributionTooLong { actual, maximum: 4096 }`.
- Story Generator prompt projection maps invalid bounded content to `StoryGeneratorProjectionError::InvalidPlayerContribution` and Turn error code `invalid_player_contribution`.
- Writer Planner and Character Think limit/invariant strings use `player_contribution`; no production error contains raw contribution text.
- SQLite migration failure is surfaced through the existing `SqliteStoreError::Migration` path and MUST NOT silently recreate or drop `story_turns`.
- No `.unwrap()` or `.expect()` may be added on request, prompt, database, or model data.

### 4.2 Concurrency

- This change adds no asynchronous task, semaphore, channel, lock, or LLM call.
- `player_contribution` remains immutable for the lifetime of one `TurnExecutionContext`.
- No lock guard may cross `.await`; existing Turn runtime and LLM limiter behavior is unchanged.

### 4.3 Observability

- Root Turn trace payload field is `player_contribution`; remove `player_input` from the active trace schema.
- Apply the same `TraceContentPolicy` and `MAX_LLM_CONTENT_CHARS` truncation as before; do not add another content-bearing span field.
- Structured error/log fields use `player_contribution` only when naming a field or limit. Production logs MUST NOT interpolate its raw value.
- Existing LLM spans and purposes remain unchanged; prompt content under content-enabled development trace naturally shows the new RC heading and slot.

---

## 5. Acceptance Criteria

### 5.1 Canonical Rename

- [ ] `MAX_PLAYER_CONTRIBUTION_CHARS`, `EmptyPlayerContribution`, `PlayerContributionTooLong`, `TurnRequest::player_contribution`, `ExecuteTurnSpec.player_contribution`, and `TurnExecutionContext::player_contribution()` match §3.2.
- [ ] `StoryTurn`, `StoryTurnView`, `TurnData`, `StoryHistoryConfig`, and `RetrievalSignalOrigin` match §3.2.
- [ ] All four prompt projections and slot definitions use only `player_contribution`.
- [ ] The active-tree legacy-name check returns zero matches:

```bash
rg -n '(player_input|PlayerInput|Player Input|player-input|playerInput)' \
  crates/aise/src \
  crates/aise/tests \
  crates/aise/assets/prompts/context-v2 \
  crates/aise-server/src \
  crates/aise-server/tests \
  crates/aise-server/assets
```

- [ ] No `serde(alias = "player_input")`, fallback prompt variable, deprecated symbol, or dual JSON field exists.

### 5.2 Prompt Contracts

- [ ] Writer Planner CSI/FTI has `10/3/5` and `5/3` rule counts and contains the §3.7 pending/goal wording — verified by `writer_planner_assets_preserve_required_rule_counts` and a dedicated semantics assertion.
- [ ] Character Think CSI/FTI has `10/3/5` and `5/3` rule counts and contains the §3.8 perceptibility/private-thought wording.
- [ ] Story Generator CSI/FTI has `10/3/5` and `5/3` rule counts; update `story_generator_assets_have_required_rule_counts` from 9 to 10 MUST.
- [ ] Story Repairer CSI/FTI has `10/3/5` and `5/3` rule counts; update `story_repairer_assets_have_required_rule_counts` from 8 to 10 MUST.
- [ ] Writer Planner, Character Think, and Story Generator RC end with exactly one `## Pending Player Contribution`; Story Repairer contains exactly one nested `### Pending Player Contribution` before `Previous Story Text`.
- [ ] Prompt Runtime Context contains the contribution marker exactly once and CSI/FTI contain it zero times — verified for all four profiles in `prompt_context_contract_tests.rs`.
- [ ] Prompt injection tests continue to prove contribution content remains RC data even when it includes Markdown headings, Jinja syntax, schema text, or fake system instructions.
- [ ] Story Repairer reuses the renamed generation context and does not introduce a second contribution field or slot.

### 5.3 Pipeline, Persistence, API, and UI

- [ ] Retrieval signals derived from the contribution serialize origin `player_contribution`; a unit test covers entity and topic signals.
- [ ] Character Think prompt tests cover a mixed contribution with observable speech/action plus private thought and assert the CSI/FTI boundary text is present.
- [ ] Migration `0022_player_contribution.sql` applies to a fresh database and produces `story_turns.player_contribution` with no live `player_input` column.
- [ ] Upgrade migration from schema version 21 preserves a seeded Turn's contribution bytes, row count, digest, idempotency key, sequence, story text, and revisions — verified by `player_contribution_migration_tests.rs`.
- [ ] `sqlite_store.rs` inserts and `sqlite_story_history_reader.rs` selects only `player_contribution`; `StoryHistoryConfig.max_player_contribution_bytes` enforces the existing 16 KiB default.
- [ ] Turn POST JSON accepts `player_contribution`; the bundled client sends that field and no active client code sends `player_input`.
- [ ] Story history JSON exposes `player_contribution` and no `player_input` — verified in `crates/aise-server/tests/story_api_tests.rs`.
- [ ] Root Turn trace JSON exposes `player_contribution` and no `player_input` under both metadata/content policy tests.
- [ ] `renderStory()` appends only opening and `turn.story_text`; Turn submission does not append `你：...` to the story panel.
- [ ] DOM id, CSS selector, JavaScript binding, request field, and placeholder match §3.4.
- [ ] Story State Extractor and deterministic Validation prompt/context contracts contain no `player_contribution` slot.

### 5.4 Behavioral Regression Matrix

Run the following cases through the actual Writer Planner and Story Generator configuration used by trace `2026-08-19-11_56_38_199.json`. Inspect the returned prose and content-enabled trace:

| Case | Contribution | Required result |
|---|---|---|
| Dialogue regression | `你是谁` | Prose shows the Player Character asking before or while the outside character responds; it does not begin at the answer |
| Action | `我后退一步，把门闩压紧` | Both actions/attempts appear before consequences; success is resolved causally |
| Thought | `我觉得门外的人在撒谎` | Suspicion appears as subjective Player Character thought; it is not asserted as fact |
| Mixed | `我后退一步，问“你是谁”，心想他可能认识我` | Action, utterance, and thought all appear; ordering may change but none is omitted |
| Blocked attempt | `我推开锁死的门` | The attempt is shown; the locked state is not silently contradicted and success is not guaranteed |
| First-person hope | `我希望门外是熟人` | The hope may appear as private thought; the stranger is not made familiar solely because of it |
| Requested external result | `让门外的人立刻投降` | The requested result is not established or guaranteed as Player Character behavior or world fact |
| Elaboration guard | `你是谁` | Added key-touching, posture, or reaction details do not replace the explicit question |

- [ ] Writer Planner's regression `story_goal` explicitly includes realizing the question, not only answering it.
- [ ] Story Generator passes all eight cases for the configured primary provider/model.
- [ ] If Story Repairer is invoked on a candidate that omits `你是谁`, its complete replacement includes the question while addressing the supplied validation issue.

### 5.5 Repository Verification

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `git diff --check` passes.
- [ ] `git status --short` shows no generated database, trace, or temporary eval artifact added by the implementation.
- [ ] Existing unrelated untracked files are preserved and not included in this refactor.

---

## 6. Out of Scope / Future Work

- A typed speech/action/thought classifier may be designed only if provider regression data shows prompt-only interpretation is insufficient.
- A semantic contribution-realization validator may be designed later as part of the full Validation/Repair architecture; it is not added while the front generation path is under focused debugging.
- Author-level player requests for desired story outcomes require a separate explicit product contract and authority channel; they must not be overloaded onto `Player Contribution`.

---

## 7. References

- Source design: [Player Contribution Realization — Design](../design/2026-08-19-player-contribution-realization-design-gpt.md)
- Prior Story Generator prompt spec: [Story Generator CSI-RC-FTI Prompt Spec](CSI-RC-FTI/2026-08-12-story-generator-csi-rc-fti-prompt-spec-gpt.md)
- Prior Character Think decision spec: [Character Think Decision Spec](CSI-RC-FTI/2026-08-14-character-think-decision-spec-gpt.md)
- Runtime Context empty-elision design: [Runtime Context Empty Elision](../design/CSI-RC-FTI/2026-08-17-runtime-context-empty-elision-design-gpt.md)
- Project guardrails: [`AGENTS.md`](../../AGENTS.md), [`doc/agents/guardrails/architecture-refactor.md`](../agents/guardrails/architecture-refactor.md), [`doc/agents/guardrails/code-organization.md`](../agents/guardrails/code-organization.md), [`doc/agents/guardrails/observability.md`](../agents/guardrails/observability.md)
- Regression evidence supplied with this task: `2026-08-19-11_56_38_199.json`
