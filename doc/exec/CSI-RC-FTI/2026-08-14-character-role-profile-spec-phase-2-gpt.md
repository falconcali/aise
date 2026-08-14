# Character Role Prompt Projection — Phase 2 Spec

> **Model**: GPT-5
> **Date**: 2026-08-14
> **Status**: Proposed
> **Source Design**: [Character Card 与 Story Role Profile](../../design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md)
> **Phase**: Phase 2 of 3 — prepared context and stage-specific Prompt views

---

## 1. Goal

Project the final Role-centered runtime into compact stage-specific CSI-RC-FTI Runtime Contexts that expose `role_id` and useful character semantics while omitting Card metadata, redundant Controller labels, Binding/storage structure, inaccessible background, and removed Prompt-only profile classifications.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Rebuild Baseline Context preparation directly from `StoryReadSnapshot.roles()` without Role/Card/Binding/state joins.
- Replace every model-visible story-local `character_id` and `role_key` with `role_id`.
- Render one natural-language `personality` field and one `speaking_style` field; render `appearance` instead of Character Card `description`.
- Remove model-visible `values`, `fears`, `speaking_register`, `speaking_verbosity`, `speaking_traits`, Card version/digest, and source `CharacterId`.
- Define exact field visibility for WriterPlanner, CharacterThink, StoryGenerator, StoryRepairer, and StoryStateExtractor.
- Keep Role background writer-facing and prevent automatic CharacterThink exposure.
- Remove redundant `control: player`/`control: ai` from sections whose controller is already fixed by the section contract.
- Update WriterPlanner output schema, retrieval audiences, CharacterThink requests, Character Decisions, Narrative impulses, and extractor schema to use Role IDs.
- Add bounded deterministic Dialogue Example selection for CharacterThink and StoryGenerator.
- Update Prompt `.md.j2` assets, slot/catalog contracts, renderers, tests, traces, and semantic evaluation fixtures.

### 2.2 Non-Goals

- Does not change the trusted CSI-RC-FTI authority boundary, Prompt catalog ownership, message roles, model selection, or tool policy.
- Does not add System Prompt, post-history instruction, Character Card instruction, or Story-authored Prompt injection points.
- Does not redesign the existing CSI MUST/SHOULD/NEVER counts or FTI checklist counts owned by the stage-specific Prompt specs; only identity/profile terminology required by this contract changes.
- Does not add a new LLM call, summarization call, profile-generation call, or background-to-memory extraction call.
- Does not expose full Role background to CharacterThink, even when the writer can see it.
- Does not render every off-scene Role with a full Profile; bounded Role indexes remain compact.
- Does not truncate Profile text into new semantic content. Optional Dialogue Examples may be omitted deterministically; required projected fields either fit or projection fails.
- Does not persist Prompt views, Character Decisions, Narrative impulses, or retrieval projections.

### 2.3 Implementation Constraints

- Phase 0 and Phase 1 are prerequisites and all three phases merge atomically.
- Prompt data is untrusted Runtime Context only. It cannot create, replace, or modify CSI, FTI, output schema, Prompt asset metadata, slot definitions, or message authority.
- Every model-visible Role target uses exact `RoleId`; name and array position are never accepted as output target identity.
- Every stage receives only the fields in the visibility matrix in §3.2. Adding another field requires updating this spec and contract tests.
- Preserve current RC section order unless this document explicitly renames a field inside a rendered section.
- Preserve the Character Decision and StoryStateExtractor semantics from their owning specs while applying the RoleId overrides in Phase 1.
- Required Prompt data must stay bounded; projection returns a typed pre-LLM error rather than silently dropping required on-scene Role data (`R-ARCH-03`, `R-ARCH-04`, `R-OBS-01`).
- All Pipeline LLM calls continue through the injected shared `LlmGateway` limiter (`R-CONC-04`).

---

## 3. Contracts

### 3.1 Prepared Role Context

Use the final Phase 1 prepared types. `BaselineContextBuilder` creates each `RoleContextView` from exactly one `StoryRoleView`:

```rust
fn project_role_context(role: &StoryRoleView) -> RoleContextView;
```

The function copies:

```text
role_id
role_label
narrative_function
background
effective_profile -> profile
state
controller
```

It never reads Character Card storage, Pack `default_profile`, or a second map. Baseline partitions are:

```rust
pub struct BaselineContext {
    pub player_role: RoleContextView,
    pub scene_roles: Vec<RoleContextView>,
    pub referenced_roles: Vec<RoleContextView>,
    pub role_index: Vec<RoleIndexEntry>,
}
```

All existing non-role Baseline Context fields remain unchanged.

- `player_role` is the sole player-controlled Role.
- `scene_roles` contains present AI Roles except `player_role`.
- `referenced_roles` contains bounded off-scene AI Roles selected by exact Role entity signals.
- `role_index` contains all remaining bounded Roles and excludes the player Role.
- Each partition is duplicate-free and sorted by `RoleId`.

### 3.2 Stage Visibility Matrix

| Field | WriterPlanner full Role | CharacterThink target | StoryGenerator / Repairer full Role | StoryStateExtractor Role state/index |
|---|:---:|:---:|:---:|:---:|
| `role_id` | Yes | Yes | Yes | Yes |
| `name` | Yes | Yes | Yes | Yes |
| conditional `role` (`role_label`) | Yes | Yes | Yes | Yes |
| `narrative_function` | Index retrieval hint only | No | No | No |
| `appearance` | Yes | Yes | Yes | No |
| `personality` | Yes | Yes | Yes | No |
| `speaking_style` | Yes | Yes | Yes | No |
| `dialogue_examples` | No | Bounded | Bounded | No |
| `background` | Yes | **No** | Yes | No |
| current `location` / `goals` / `attributes` | Yes | Yes | Yes | Yes |
| `controller` | No in fixed-controller sections | No | No in fixed-controller sections | No |
| source `CharacterId` / version / digest | No | No | No | No |
| default Profile | No | No | No | No |

The Role index is intentionally smaller than a full Role projection:

```text
target_id
role_id
name
conditional role
retrieval_hint
```

It contains no Profile, background, state map, source Card metadata, or Controller field.

### 3.3 Shared Rendered Role Shape

Writer-facing full Role rendering uses this field order:

```text
role_id: "protagonist"
name: "The Traveler"
role: "An amnesiac traveler who woke in the lodge"
appearance: "A mud-stained dark travel coat."
personality: "Cautious and curious; values truth and personal safety."
speaking_style: "Concise and probing; rarely reveals conclusions."
background: "Entered the Grey Wood last night; the experience is now fragmented."
location: "lodge_hall"
goals: ["Learn why they are here"]
attributes: {"health": 10, "sanity": 8}
```

Rendering rules:

- `role_id` and `name` are always present.
- Omit the `role` line when `role_label.as_str() == profile.name.as_str()`; never emit an empty or duplicate Role label.
- Omit absent optional `appearance`, `personality`, `speaking_style`, and `background` lines. Do not render them as empty strings, `null`, or inherited values.
- Preserve exact validated text; JSON-quote every string scalar.
- Render goals in stored order and attributes in `BTreeMap` key order.
- `background` is included only in writer-facing projections.
- Never render `controller`, `source_character_id`, Card version/digest, or the Role default Profile in a full fixed-controller section.
- Collection entries use `- role_id:` for the first line and two-space indentation for following fields.

### 3.4 WriterPlanner Projection

Replace the Prompt-context identity types:

```rust
#[derive(Debug, Clone)]
pub struct WriterPlannerPromptContext {
    pub role_targets: BTreeMap<RetrievalTargetId, RoleId>,
    pub knowledge_targets: BTreeMap<RetrievalTargetId, KnowledgeSourceId>,
    pub provided_role_ids: Vec<RoleId>,
    pub provided_knowledge_ids: Vec<KnowledgeSourceId>,
}
```

Runtime sections retain this order:

```text
Story Profile
Instance Settings
Story Continuity / Story Summary
Story Continuity / Recent Story
Current Scene
Player Character
Scene Characters
Referenced Characters
Relevant Knowledge
Character Index
Knowledge Entry Index
Narrative Plan
Active Story Constraints
Player Input
```

Role section rules:

- `Player Character` renders the writer-facing full Role shape without `control: player` and without Dialogue Examples.
- `Scene Characters` and `Referenced Characters` render AI full Role shapes without `control: ai` and without Dialogue Examples.
- Referenced Roles retain `presence: referenced_off_scene` after the full Role fields.
- Character Index retains its existing heading for model clarity but uses the compact Role index shape and `role:{role_id}` target IDs.
- `narrative_function` appears only as the Character Index `retrieval_hint`.
- Narrative Character impulses render `target_role_id`, never Character ID.

The WriterPlanner output schema uses:

```json
{
  "audience": {
    "kind": "character",
    "role_id": "guard_captain"
  },
  "target_id": "role:guard_captain",
  "query_text": null,
  "reason": "The guard captain may act on a private memory."
}
```

```json
{
  "character_think_requests": [
    {
      "role_id": "guard_captain",
      "reason": "Decide whether to conceal the warning."
    }
  ]
}
```

Schema requirements:

- Character audience requires exactly `kind` and `role_id`.
- CharacterThink request requires exactly `role_id` and `reason`.
- `character_id` is an unknown field at every WriterPlanner output location.
- Exact target validation resolves `role:{role_id}` through `WriterPlannerPromptContext.role_targets`.

### 3.5 CharacterThink Projection

Replace target/profile view contracts with:

```rust
#[derive(Debug, Clone)]
pub struct CharacterThinkRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone)]
pub struct CharacterThinkStatePromptView {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: Vec<CharacterStateAttributePromptView>,
}
```

`Target Character` renders in this order:

```text
role_id
name
conditional role
appearance, if present
personality, if present
speaking_style, if present
dialogue_examples, if selected
```

CharacterThink rules:

- Resolve target only from `CharacterThinkRequest.role_id` against present/direct-participant AI Roles.
- Do not render `background`, `narrative_function`, Controller, source Character metadata, another Role’s Profile, or another Role’s private Knowledge.
- Story Summary and Recent Story remain contextual narrative text and do not grant knowledge by themselves.
- Relevant private Knowledge contains only target-authorized Rumors and target-owned Memories using RoleId ownership.
- Narrative Character impulses compare `target_role_id`.
- Thinking Focus remains the validated request reason under the existing Character Decision spec; it does not grant knowledge or force an outcome.
- CharacterDecision output schema still contains only `decision` and optional nullable `suggested_utterance`; the engine binds `role_id` after validation.

### 3.6 StoryGenerator and StoryRepairer Projection

Replace StoryGenerator Role/Profile views with:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub presence: CharacterPresence,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    pub dialogue_examples: Vec<DialogueExample>,
    pub background: Option<BoundedText>,
    pub state: StoryGeneratorRoleStatePromptView,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorCharacterDecisionPromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
```

- Delete `CharacterControl` because player and AI Roles are already split into `Player Character` and `AI Characters` sections.
- `Player Character` and `AI Characters` render the writer-facing full Role shape plus bounded Dialogue Examples.
- AI Role `presence` remains model-visible only when it distinguishes `scene` from `referenced`; do not render a redundant fixed `control` field.
- Current Scene renders `present_role_ids`.
- `AI Character Decisions` renders `role_id`, name, decision, and optional suggested utterance in validated request order.
- `character_id` is absent from the view, renderer, errors, schema, and tests.
- StoryRepairer reuses the exact StoryGenerator projection and Character Decisions from the current candidate version. It must not re-project from Character Card storage or create a second Role/Profile path.
- StoryRepairer’s previous candidate and validation sections remain governed by the StoryStateExtractor split spec.

### 3.7 StoryStateExtractor Projection

The extractor receives only the Role data required to bind final state:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractorRolePromptView {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}
```

- It receives all bounded existing Roles required by the candidate story and final-state contract, ordered by RoleId.
- It receives no appearance, personality, speaking style, Dialogue Examples, background, Controller, source Character metadata, default Profile, or Character Decision.
- Its output schema uses `role_states`, `role_id`, Relationship Role fields, Memory owner RoleId, Rumor `source_role_id`, and Current Scene `present_role_ids` from Phase 1.
- Unknown `character_id`, `role_key`, and `character_states` fields fail schema validation.

### 3.8 Dialogue Example Selection

Add to `ContextPreparationConfig` and `config/aise_config.toml`:

```rust
pub struct ContextPreparationConfig {
    pub max_dialogue_examples_per_role: usize,
    pub max_dialogue_example_tokens_per_role: u64,
}
```

All existing fields remain with the Phase 1 Role renames.

Defaults:

```text
max_dialogue_examples_per_role = 4
max_dialogue_example_tokens_per_role = 256
```

Selection algorithm:

1. WriterPlanner and StoryStateExtractor select zero Dialogue Examples.
2. CharacterThink and StoryGenerator start from the Profile’s stored example order.
3. Take at most `max_dialogue_examples_per_role` from the beginning.
4. Append an example only when cumulative `estimate_text_tokens(situation) + estimate_text_tokens(response)` remains within the per-Role token limit.
5. Preserve selected example text and order exactly; never truncate an example.
6. If the overall required Prompt exceeds its stage token budget, remove selected Dialogue Examples from the end, Role by Role in descending RoleId order, until they fit.
7. If the Prompt still exceeds budget after all Dialogue Examples are removed, return `required_prompt_data_exceeds_budget` before the LLM call.

Both configuration values must be positive. No old key aliases are accepted.

### 3.9 Runtime Variable and Template Contracts

Keep user-facing section terminology (`Player Character`, `Scene Characters`, `AI Characters`, `Character Index`, `Target Character`, `AI Character Decisions`) because these describe narrative actors, not ID/storage domains.

Required runtime variables:

| Stage | Role-related variables |
|---|---|
| WriterPlanner | `player_character`, `scene_characters`, `referenced_characters`, `character_index`, `narrative_plan` |
| CharacterThink | `target_character`, `current_character_state`, `relevant_character_knowledge`, `narrative_character_impulses` |
| StoryGenerator | `player_character`, `ai_characters`, `character_decisions`, `current_scene` |
| StoryRepairer | exact StoryGenerator variables plus repair-owned candidate/issues |
| StoryStateExtractor | extractor-owned `roles`, `current_scene`, candidate story, Knowledge views, valid keys |

- Variable names remain semantic and do not imply a storage identity.
- Rendered target fields inside those variables use `role_id` only.
- Remove `character_thoughts` if the Character Decision spec has not already done so; only `character_decisions` remains.
- All `.md.j2` RC templates remain data-only. Profile/background values are inserted only through Runtime variables.
- `index.yaml` and `slots.yaml` require the final variable names and final output contract references; no alias slot accepts legacy variables.

### 3.10 Prompt Rule Updates

Apply only these semantic wording changes to trusted Prompt assets:

- “exact character ID” becomes “exact Role ID (`role_id`)” where the instruction names a wire field.
- WriterPlanner is reminded that names are non-unique display text and all requested targets use exact Role IDs.
- CharacterThink is reminded that Role background is not automatically known and only supplied Knowledge/Memory can grant private facts.
- StoryGenerator/Repairer are reminded that `role_id` identifies the actor and source Character Card metadata is irrelevant.
- StoryStateExtractor is required to output only known exact Role IDs from its Role view.

Do not add new rule bullets merely to restate rendered field names. Preserve the exact MUST/SHOULD/NEVER and FTI counts required by each stage’s existing owning spec.

### 3.11 Projection Errors

Rename Role-identity errors and use exact structured fields:

```rust
pub enum WriterPlannerProjectionError {
    UnknownRoleTarget { role_id: RoleId },
    PlayerRoleTarget { role_id: RoleId },
    DuplicateRoleTarget { role_id: RoleId },
    RequiredPromptDataExceedsBudget,
}

pub enum CharacterThinkProjectionError {
    UnknownRole { role_id: RoleId },
    PlayerControlledRole { role_id: RoleId },
    RoleNotPresent { role_id: RoleId },
    UnauthorizedKnowledge { role_id: RoleId },
    RequiredPromptDataExceedsBudget,
}

pub enum StoryGeneratorProjectionError {
    UnknownDecisionRole { role_id: RoleId },
    PlayerRoleDecision { role_id: RoleId },
    DuplicateRoleDecision { role_id: RoleId },
    RequiredPromptDataExceedsBudget,
}
```

All existing non-role projection-error variants remain unchanged.

No error message includes Profile, background, Dialogue Example, Memory, Decision, Story Continuity, or Player Input text.

### 3.12 File and Directory Layout

```text
crates/aise/src/
├── context/
│   └── baseline_ctx_builder.rs
├── planning/
│   ├── writer_planner_prompt.rs
│   └── tests/writer_planner_tests.rs
├── character/
│   ├── character_think_prompt.rs
│   └── tests/character_think_prompt_tests.rs
├── story/
│   ├── story_generator_prompt.rs
│   ├── story_repairer_prompt.rs
│   ├── story_state_extractor_prompt.rs
│   └── tests/
│       ├── story_generator_prompt_tests.rs
│       ├── story_repairer_prompt_tests.rs
│       └── story_state_extractor_prompt_tests.rs

crates/aise/assets/prompts/context-v2/
├── csi/
├── rc/
├── fti/
├── index.yaml
└── slots.yaml

crates/aise/tests/
└── prompt_context_contract_tests.rs
```

Use the extractor module created by the StoryStateExtractor split spec; do not create a second extractor module or Prompt projector.

---

## 4. Behavior Rules

1. **CRP2-ID-01**: Every model-visible Story actor target is `role_id`; Card `character_id`, `role_key`, and name targeting are prohibited.
2. **CRP2-VIEW-01**: Every stage receives only fields marked “Yes” in §3.2.
3. **CRP2-VIEW-02**: Source Character Card ID/version/digest and default Profile never enter Runtime Context.
4. **CRP2-VIEW-03**: Fixed-controller sections omit Controller; a mixed collection may render Controller only after a separate explicit contract change.
5. **CRP2-PROFILE-01**: `appearance`, `personality`, and `speaking_style` each render as at most one natural-language field and are omitted when absent.
6. **CRP2-PROFILE-02**: `values`, `fears`, register, verbosity, and traits never reappear as separate fields.
7. **CRP2-PROFILE-03**: A rendered Role uses only its frozen Effective Profile; it never consults or combines the Role default Profile or Character Card library.
8. **CRP2-BG-01**: WriterPlanner, StoryGenerator, and StoryRepairer may receive Role background; CharacterThink and StoryStateExtractor do not.
9. **CRP2-BG-02**: Writer-visible background is not proof of Character knowledge and cannot enter private Knowledge implicitly.
10. **CRP2-EXAMPLE-01**: Dialogue Examples are optional, ordered, count/token-bounded, and pruned only by §3.8.
11. **CRP2-INDEX-01**: Character Index remains compact and uses exact `role:{role_id}` targets; it never expands every off-scene Profile.
12. **CRP2-DECISION-01**: Character Decisions bind and render by RoleId in validated request order.
13. **CRP2-EXTRACT-01**: StoryStateExtractor receives Role state identity but no character Profile/background/private decision data.
14. **CRP2-TRUST-01**: All Profile, background, Decision, continuity, and Player Input strings remain RC data and cannot alter trusted Prompt assets or output schemas.
15. **CRP2-BOUND-01**: Required Prompt overflow fails before the LLM call; no required Role field is silently truncated or dropped.

### 4.1 Error Handling

- Unknown, duplicate, player-ineligible, or unauthorized Role targets fail with the typed Role errors in §3.11 before the affected LLM call.
- Invalid model-returned `character_id`, `role_key`, name target, or unknown `role_id` fails closed; no fallback target is selected.
- Required-context overflow returns stage-specific `TurnFailureKind::InvariantViolation`, code `required_prompt_data_exceeds_budget`, and the owning stage.
- Model output Role shape violations use the existing `model_output_invalid` LLM failure mapping.
- No projection error silently becomes `None`, an empty Profile, an empty Decision, or a skipped requested Role.

### 4.2 Concurrency

- Projection is synchronous over bounded prepared data and performs no Store, Character library, network, or LLM call.
- CharacterThink retains its existing bounded execution order from the Character Decision spec; this change adds no fan-out.
- All stage calls continue through `LlmGateway`, and no lock guard is held across `.await`.
- Sibling CharacterThink calls cannot observe sibling Decisions or private contexts.

### 4.3 Observability

- Rename story-runtime trace fields to `role_id`, `role_count`, `provided_role_count`, `decision_role_count`, and `role_audience_count`.
- Character Card service traces may retain `character_id`; Prompt/Pipeline traces may not.
- Record selected Dialogue Example count/token cost, omitted example count, Prompt section byte counts, projection duration, render duration, model duration, and parse status.
- Never record Profile text, Role background, Dialogue Example text, Memory, Decision text, Story Continuity, Player Input, or full rendered RC in production telemetry.
- Existing development-only full-content tracing remains governed by the current trusted configuration and redaction policy; this spec does not broaden it.

---

## 5. Acceptance Criteria

### Baseline and Stage Views

- [ ] Baseline Context resolves every Role from `snapshot.roles()` with no Card/Binding/definition/state join.
- [ ] Player, scene, referenced, and index partitions are RoleId-sorted, duplicate-free, bounded, and mutually exclusive.
- [ ] Stage projections match the exact visibility matrix in §3.2.
- [ ] Duplicate names render with distinct Role IDs and all model target validation remains unambiguous.
- [ ] Card source metadata and default Profile never appear in any Runtime Context variable.

### WriterPlanner

- [ ] Full Role rendering follows §3.3 and omits fixed Controller and Dialogue Examples.
- [ ] Character Index uses `role:` target IDs, `role_id`, name, conditional Role label, and retrieval hint only.
- [ ] Output schema accepts `{kind: character, role_id}` and CharacterThink `{role_id, reason}` only.
- [ ] `character_id`, `role_key`, `control`, removed Profile classifications, and Card metadata are absent.

### CharacterThink

- [ ] Target view contains Role ID, Profile voice/personality fields, bounded Dialogue Examples, and current state.
- [ ] Target view contains no background, Narrative function, Controller, source Card metadata, sibling Profile, sibling Decision, or unauthorized Knowledge.
- [ ] Memory authorization and Narrative impulses compare exact Role IDs.
- [ ] CharacterDecision schema still omits model-supplied ID; engine-bound Role ID is tested.

### Generator, Repairer, and Extractor

- [ ] StoryGenerator/Repairer full Roles use Effective Profile and writer-visible background, with no fixed Controller or Card metadata.
- [ ] `AI Character Decisions` renders `role_id` and preserves validated request order.
- [ ] StoryRepairer reuses the exact generation projection and candidate-bound decisions.
- [ ] StoryStateExtractor Role view contains state identity only and its schema uses every Phase 1 Role field name.
- [ ] No extractor Runtime Context contains Profile, background, Character Decision, or source Card metadata.

### Prompt Assets, Bounds, and Trust

- [ ] Every existing stage-specific CSI/FTI rule count remains equal to its owning Prompt spec after terminology edits.
- [ ] RC section order remains unchanged for WriterPlanner, CharacterThink, StoryGenerator, and StoryRepairer.
- [ ] Dialogue Example count/token selection and deterministic budget pruning match §3.8.
- [ ] Required Prompt overflow fails before LLM invocation.
- [ ] Injection-like strings in every Profile/background field remain RC data and cannot modify CSI, FTI, output schema, slots, or message roles.

### Hard-Refactor Verification

- [ ] `rg -n '"character_id:|"role_key:|"story_role:|"control: (player|ai)|"values:|"fears:|"speaking_register:|"speaking_verbosity:|"speaking_traits:' crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/assets/prompts/context-v2` returns zero legacy rendered fields.
- [ ] `rg -n 'character_thoughts|AI Character Thoughts|target_character_id|present_character_ids' crates/aise/src crates/aise/assets/prompts/context-v2` returns zero matches.
- [ ] `rg -n 'source_character_id|character_key|card_version|card_digest' crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/assets/prompts/context-v2` returns zero Prompt-path matches.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Required Tests

### 6.1 Shared Role Rendering

Test required fields, every optional field present/absent, Role label equal/different from name, JSON escaping, goals order, attribute order, collection indentation, duplicate names, and no source metadata.

### 6.2 Visibility Matrix Tests

Use one fixture containing every Role field and assert exact inclusion/exclusion separately for WriterPlanner, CharacterThink, StoryGenerator, StoryRepairer, StoryStateExtractor, and Character Index.

The fixture must include a secret background fact not present in Memory. CharacterThink output context must not contain it; writer-facing contexts must contain it exactly once.

### 6.3 Identity and Output-Schema Tests

Test `role:` target resolution, Character audience JSON, CharacterThink request JSON, unknown Role, duplicate Role, player Role, name-as-ID rejection, old `character_id`/`role_key` rejection, CharacterDecision engine binding, and extractor Role schema.

### 6.4 Dialogue Example Tests

Test zero examples, exact count limit, one-over count, exact token limit, one-over token omission, stored-order preservation, no text truncation, deterministic end-pruning under global budget, and required-data overflow after all examples are removed.

### 6.5 Trust and Redaction Tests

Place instruction-like text in name, appearance, personality, speaking style, Dialogue Examples, and background. Assert it appears only in RC data, cannot add/change output fields, cannot select a Prompt asset, and is absent from production error/log payloads.

### 6.6 Semantic Evaluation Matrix

Run these cases against configured models without adding production keyword checks:

| Case | Required result |
|---|---|
| two Roles share a name | planner/think/generator bind by RoleId without identity drift |
| Role uses default Profile | voice/personality come only from default Profile |
| Role uses Card Profile missing appearance | appearance stays absent; default appearance is not merged |
| Card edited after Story creation | existing Story uses frozen old Effective Profile |
| background contains a secret; Memory does not | writer may use it for authorship; CharacterThink does not know it |
| Memory contains the secret | only the owning Role may use it in CharacterThink |
| selected Dialogue Examples exceed budget | deterministic suffix examples are omitted without truncation |
| prompt injection in speaking style | text affects style only and cannot alter instructions/schema |
| player and AI Role sections | no redundant Controller labels are rendered |
| extractor final state | all actor targets use exact Role IDs and no Profile metadata appears |

---

## 7. Out of Scope / Future Work

- Adaptive semantic compression of very long Profile fields requires a separate deterministic design; this spec uses validation and existing Prompt budgets.
- Mixed-controller character collections would require an explicit Controller field contract; current fixed sections do not need it.
- CharacterThink access to selected background fragments would require explicit knowledge projection, not a visibility flag on the full background.
- Dialogue Example relevance ranking may be designed later; this spec preserves author order.

---

## 8. References

- Source design: `doc/design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md`.
- Phase 0: `doc/exec/character-role-profile-spec/2026-08-14-character-role-profile-spec-phase-0-gpt.md`.
- Phase 1: `doc/exec/character-role-profile-spec/2026-08-14-character-role-profile-spec-phase-1-gpt.md`.
- WriterPlanner Prompt spec: `doc/exec/CSI-RC-FTI/2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md`.
- Character Decision spec: `doc/exec/CSI-RC-FTI/2026-08-14-character-think-decision-spec-gpt.md`.
- StoryGenerator Prompt spec: `doc/exec/CSI-RC-FTI/2026-08-12-story-generator-csi-rc-fti-prompt-spec-gpt.md`.
- StoryRepairer Prompt spec: `doc/exec/CSI-RC-FTI/2026-08-13-story-repairer-csi-rc-fti-prompt-spec-gpt.md`.
- StoryStateExtractor spec: `doc/exec/CSI-RC-FTI/2026-08-14-story-state-extractor-split-spec-gpt.md`.
- Current WriterPlanner rendering: `crates/aise/src/planning/writer_planner_prompt.rs:278`.
- Current CharacterThink projection: `crates/aise/src/character/character_think_prompt.rs:118`.
- Current StoryGenerator profile rendering: `crates/aise/src/story/story_generator_prompt.rs:643`.
- Current RC assets: `crates/aise/assets/prompts/context-v2/rc/`.
- Project guardrails: `AGENTS.md` and `doc/agents/guardrails/`.
