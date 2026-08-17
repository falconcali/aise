# Story Context Simplification — Spec

> **Model**: GPT-5
> **Date**: 2026-08-17
> **Status**: Proposed
> **Source Design**: [Story Context Simplification](../design/2026-08-17-story-context-simplification-design-gpt.md)
> **Phase**: N/A

---

## 1. Goal

Remove `StoryProfile.premise` end to end and render Story Summary plus Recent Story as unwrapped narrative prose while retaining all internal continuity metadata.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Replace Story Pack v4 with Story Pack v5 and delete `StoryProfile.premise` from the asset schema.
- Delete Premise from snapshots, prompt projections, token estimation, persistence checks, HTTP views, the built-in UI, examples, and all valid fixtures; keep only a negative validation fixture that proves the removed key is rejected.
- Render `StorySummary.text` directly as prose and render ordered Recent Story segment bodies as one prose block separated by blank lines.
- Remove model-visible `sequence`, `text`, list markers, JSON quoting, and `None.` sentinels from Story Continuity.
- Conditionally omit empty Story Summary, Recent Story, and Story Continuity headings in all four consumer RC templates.
- Add the migration guard, validation changes, API break, and tests required by the final contract.

### 2.2 Non-Goals

- Does not delete or change `StorySegment.sequence`, `StorySegment.origin`, `StorySegment.text`, `StorySummary.summarized_through`, `StoryContinuity`, or their persistence format.
- Does not merge Story Summary and Recent Story into one field or one semantic section.
- Does not implement Story Summary generation, change summary quality, resize the Recent Story window, or change continuity compaction.
- Does not delete or change `StoryPackMeta.description`, `StoryStart.scene_key`, `StoryStart.location_key`, `StoryStart.time`, `StoryStart.description`, or `StoryStart.opening`.
- Does not inject `StoryPackMeta.description`, `StoryStart.description`, or `StoryStart.opening` as a replacement Prompt Premise.
- Does not reimplement any change owned by [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md).
- Does not provide automatic Story Pack v4 conversion, serde aliases, default Premise values, compatibility API fields, or in-place persisted Pack rewriting.

### 2.3 Implementation Constraints (for code generation)

- Implement this spec after `2026-08-17-current-scene-removal-spec-gpt.md`; that spec owns migration `0018`, and this spec owns migration `0019`.
- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, deprecated fields, aliases, adapters, or dual-version Story Pack parsing.
- Old types, fields, renderers, branches, fixtures, API properties, UI elements, and tests superseded by this spec MUST be deleted in the same change.
- Do not mutate historical migrations or rewrite stored canonical Pack JSON. Migration `0019` is a fail-fast guard, not a data converter.
- Keep `content.max_story_profile_bytes`; it continues to bound the complete serialized Story Profile after Premise removal.
- `R-ARCH-01/03/04/05`, `R-REFACTOR-01/02`, `R-CODE-01/02/05/07`, `R-LAYER-01`, and `R-AISE-01/02/03` remain mandatory.

---

## 3. Contracts

### 3.1 Story Pack v5 Types

`AssetSpecVersion` remains shared by asset families and adds `V5_0`; existing variants remain for Character Card and World Book contracts:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSpecVersion {
    #[serde(rename = "3.0")]
    V3_0,
    #[serde(rename = "4.0")]
    V4_0,
    #[serde(rename = "5.0")]
    V5_0,
}
```

`StorySpec` accepts only the new Story Pack discriminator:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorySpec {
    #[serde(rename = "aise_story_v5")]
    V5,
}
```

`StoryProfile` has exactly these fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProfile {
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub style: StoryStyle,
}
```

The required Story Pack v5 discriminator and Story Profile fragment is:

```json
{
  "spec": "aise_story_v5",
  "spec_version": "5.0",
  "story": {
    "language": "zh-CN",
    "genre": ["mystery"],
    "themes": ["discovery"],
    "style": {
      "tone": ["suspenseful"],
      "point_of_view": "second",
      "tense": "present"
    }
  }
}
```

`StoryPackMeta` and `StoryStart` retain their post-Current-Scene-Removal contracts without added fields.

### 3.2 Story Pack Validation Protocol

`NativeAssetImporter::validate_pack_value` keeps its signature:

```rust
pub fn validate_pack_value(
    &self,
    value: &serde_json::Value,
    report: &mut ValidationReport,
);
```

It MUST apply these checks in order:

```text
1. /spec must equal "aise_story_v5".
2. /spec_version must equal "5.0".
3. /story/premise must be absent.
4. The complete value must deserialize as StoryPack with deny_unknown_fields.
5. Existing semantic Role, Start, Narrative, reference, size, and salience validation runs.
```

Validation failures use these exact report entries:

| Condition | Code | Path | Message |
|---|---|---|---|
| `spec` is `aise_story_v4` or any other value | `AssetValidationCode::UnsupportedSpec` | `/spec` | `unsupported spec {value}` |
| `spec_version` is not `5.0` | `AssetValidationCode::UnsupportedSpecVersion` | `/spec_version` | `unsupported spec_version {value}` |
| `/story/premise` exists under a valid v5 discriminator pair | `AssetValidationCode::SchemaInvalid` | `/story/premise` | `premise is not supported` |
| Final v5 JSON does not deserialize as `StoryPack` | `AssetValidationCode::SchemaInvalid` | `/` | `pack JSON does not match the final schema` |

The importer MUST return immediately after a discriminator error. If Premise is present, it MUST record the exact Premise error and return before the general schema check so the diagnostic path is exact. `PackService::import` MUST never persist a report with `valid == false`.

### 3.3 Prompt Projection Types

`StoryProfilePromptView` contains only writing requirements:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryProfilePromptView {
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}
```

`StoryContinuityPromptView` projects segment bodies only:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryContinuityPromptView {
    pub story_summary: BoundedText,
    pub recent_story: Vec<BoundedText>,
}
```

`CharacterThinkStoryContinuityPromptView` retains the same body-only shape:

```rust
#[derive(Debug, Clone)]
pub struct CharacterThinkStoryContinuityPromptView {
    pub story_summary: BoundedText,
    pub recent_story: Vec<BoundedText>,
}
```

Delete this type and every import, construction, export, and test reference:

```rust
RecentStoryPromptView
```

StoryGenerator projection MUST map each `StorySegment` to `segment.text.clone()` in the existing Domain order. WriterPlanner continues to read the Baseline `StoryContinuity` directly. CharacterThink retains its existing body-only projection. StoryRepairer continues to extend StoryGenerator runtime variables and MUST NOT create a separate continuity projection.

### 3.4 Story Continuity Rendering Protocol

The three prompt projector modules MUST replace `render_optional_text` with a summary-specific renderer:

```rust
fn render_story_summary(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        value.to_owned()
    }
}
```

StoryGenerator and CharacterThink use the same body-only Recent Story contract:

```rust
fn render_recent_story(values: &[BoundedText]) -> String {
    values
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}
```

WriterPlanner keeps its existing Baseline-based signature and applies the identical join:

```rust
fn render_recent_story(baseline: &BaselineContext) -> String {
    baseline
        .story_continuity
        .recent_segments()
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}
```

For these two Runtime Context variables only:

```text
story_summary = exact non-empty StorySummary.text, otherwise ""
recent_story = exact segment texts in existing order, joined by "\n\n", otherwise ""
```

The renderers MUST NOT trim non-empty content, reorder segments, escape line breaks, serialize JSON, add quotes, add bullets, add field labels, expose sequence/origin, or output `None.`.

`render_story_profile` in WriterPlanner and StoryGenerator/StoryRepairer MUST render exactly these keys in this order:

```text
language
genre
themes
tone
point_of_view
tense
```

The profile remains structured because these are independent writing controls; it MUST contain no replacement synopsis, opening, current situation, or Premise field.

### 3.5 RC Template Protocol

Keep `story_summary` and `recent_story` as required string variables in these slots:

```text
context.writer_planner.rc
context.character_think.rc
context.story_generator.rc
context.story_repairer.rc
```

The three top-level RC templates MUST use this conditional structure at the existing Story Continuity position:

```jinja
{% if story_summary or recent_story %}
## Story Continuity

{% if story_summary %}
### Story Summary

{{ story_summary }}
{% endif %}
{% if recent_story %}
### Recent Story

{{ recent_story }}
{% endif %}
{% endif %}
```

StoryRepairer MUST use the same conditions with its existing nested heading levels:

```jinja
{% if story_summary or recent_story %}
### Story Continuity

{% if story_summary %}
#### Story Summary

{{ story_summary }}
{% endif %}
{% if recent_story %}
#### Recent Story

{{ recent_story }}
{% endif %}
{% endif %}
```

Whitespace-control markers MAY be added only to prevent extra blank lines; they MUST NOT alter prose bytes. Rendering rules are:

| Summary | Recent | Required headings |
|---|---|---|
| non-empty | non-empty | Continuity, Summary, Recent |
| non-empty | empty | Continuity, Summary |
| empty | non-empty | Continuity, Recent |
| empty | empty | none of the three |

### 3.6 Runtime, Persistence, API, and UI Protocol

Delete the Premise token term from `BaselineContext::estimate_tokens`:

```rust
estimate_text_tokens(self.story_profile.premise.as_str())
```

Do not add a replacement term. Existing prompt-specific final budget calculation remains authoritative.

`SqliteSnapshotProvider` MUST retain the serialized `story_profile_json` length check against `max_story_profile_bytes` and delete only the field-specific check that reads `story_profile.premise`. Newly imported v5 rows serialize the reduced `StoryProfile` shape.

After the prior Current Scene API break, `StoryView` MUST have exactly this shape:

```rust
#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub player_role_id: String,
    pub opening: Option<StoryOpeningView>,
    pub turns: Vec<StoryTurnView>,
    pub next_turn_after: Option<u64>,
    pub roles: Vec<RoleStateView>,
}
```

`get_story` MUST not read `snapshot.story_profile().premise`. `StoryInstanceView` is unchanged from the final Current Scene Removal contract.

The built-in web app MUST:

- delete the Story Pack detail row labelled `前提`;
- delete every access to `story.premise` or `pack.story.premise`;
- continue to render `meta.description`, Story Profile writing controls, `start.description`, and `start.opening`;
- add no computed or empty compatibility Premise.

### 3.7 Persistence Migration Protocol

Add exactly one migration after Current Scene migration `0018`:

```text
crates/aise/assets/persistence/mig/0019_story_context_simplification.sql
```

with this fail-fast guard:

```sql
CREATE TEMP TABLE story_context_simplification_migration_guard (
    value INTEGER CONSTRAINT story_context_simplification_legacy_data_present CHECK (value = 0)
);

INSERT INTO story_context_simplification_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances);

DROP TABLE story_context_simplification_migration_guard;
```

The migration MUST NOT delete rows, remove JSON keys, rewrite `pack_json`, rewrite `manifest_json`, change `digest`, or update `story_profile_json`. An upgraded database containing legacy Story Packs or Instances MUST fail with the named constraint. Operators must recreate the development database or remove legacy Story Pack data before retrying, then re-import v5 assets through `PackService`.

Fresh databases and empty upgraded databases MUST apply migrations `0018` and `0019` successfully.

### 3.8 File / Directory Layout

```text
crates/aise/
├── assets/persistence/mig/0019_story_context_simplification.sql
├── assets/prompts/context-v2/
│   ├── slots.yaml
│   └── rc/
│       ├── writer-planner.md.j2
│       ├── character-think.md.j2
│       ├── story-generator.md.j2
│       └── story-repairer.md.j2
├── src/domain/asset/
│   ├── character_card.rs
│   └── story_pack.rs
├── src/domain/turn/baseline.rs
├── src/planning/writer_planner_prompt.rs
├── src/character/character_think_prompt.rs
├── src/story/
│   ├── pack_service.rs
│   └── story_generator_prompt.rs
└── src/persistence/sqlite_snapshot.rs

crates/aise-server/
├── src/api/story.rs
├── assets/app.js
└── tests/

examples/demo_pack.json
```

Update existing dedicated unit-test files, integration-test fixtures, and server fixtures in the same change. Do not create inline Rust test modules.

---

## 4. Behavior Rules

1. **SCS-1 — Story Pack version**: Story Pack import, validation, examples, and fixtures MUST use only `aise_story_v5` paired with `5.0`; v4 Story Packs are unsupported.
2. **SCS-2 — Premise deletion**: No asset type, runtime type, snapshot, prompt view, API response, UI model, config, example, or valid test fixture may contain a field named `premise`; a negative importer fixture may contain the removed JSON key only to verify rejection.
3. **SCS-3 — No replacement**: The implementation MUST NOT rename Premise, derive it from Opening/Summary, copy `meta.description` into Prompt context, or introduce an equivalent synopsis field into `StoryProfile`.
4. **SCS-4 — Summary prose**: A non-empty `StorySummary.text` MUST enter RC byte-for-byte as the value of `story_summary`; an empty or whitespace-only Summary MUST project as an empty string.
5. **SCS-5 — Recent prose**: Recent Story MUST contain only `StorySegment.text` bodies in their existing oldest-to-newest order, joined with the exact delimiter `\n\n` without normalizing bytes inside either body.
6. **SCS-6 — No model metadata**: `StorySegment.sequence`, `StorySegment.origin`, `StorySummary.summarized_through`, and the field name `text` MUST never be rendered inside Story Continuity.
7. **SCS-7 — Separate authority bands**: Summary and Recent Story MUST remain separate subsections; when they conflict, the newer Recent Story is authoritative.
8. **SCS-8 — Empty sections**: Empty Summary or Recent Story subsections MUST be omitted; if both are empty, the enclosing Story Continuity section MUST be omitted.
9. **SCS-9 — Internal continuity**: Domain validation, persistence, pagination, compaction boundaries, and ordering MUST continue to use Sequence and `summarized_through` exactly as before.
10. **SCS-10 — Profile semantics**: Story Profile Prompt rendering MUST contain only language, genre, themes, tone, point of view, and tense.
11. **SCS-11 — Cross-profile consistency**: WriterPlanner, CharacterThink, StoryGenerator, and StoryRepairer MUST produce the same Summary and Recent Story bytes for the same `StoryContinuity`.
12. **SCS-12 — Repair inheritance**: StoryRepairer MUST receive the original generation continuity variables and MUST NOT serialize, quote, number, or reconstruct them independently.
13. **SCS-13 — Runtime data boundary**: Raw story prose MUST remain in RC only. It MUST never enter CSI or FTI, and inserted prose MUST not be recursively evaluated as MiniJinja syntax.
14. **SCS-14 — Immutable Pack identity**: No migration or hydration path may silently convert v4 Pack content while retaining its old Digest or Pack identity.
15. **SCS-15 — API break**: Story and Pack responses omit Premise completely; no `null`, empty, deprecated, or derived compatibility property is returned.
16. **SCS-16 — Bounded work**: The change adds no LLM call, unbounded history scan, queue, task, lock, dependency, or extra copy of complete Story Continuity beyond the existing Prompt projection.

### 4.1 Error Handling

- `NativeAssetImporter` MUST return the exact validation entries in §3.2 and MUST NOT panic or call `unwrap()` on imported JSON.
- `PackService::import` MUST return the existing `AssetImportError::Invalid(report)` for a v4 discriminator or removed Premise field.
- Migration `0019` MUST fail rather than mutate legacy Pack rows; the SQLite error MUST retain the named constraint `story_context_simplification_legacy_data_present`.
- Existing typed `StoreError`, `AssetImportError`, and `PromptError` paths remain unchanged; no rendering branch may silently substitute `None.` after this change.

### 4.2 Concurrency

- No new asynchronous work, lock, task, queue, or LLM request is introduced.
- Pack import and database migration retain their existing transaction boundaries.
- Existing LLM calls remain routed through `LlmGateway` and its shared concurrency limiter.

### 4.3 Observability

- Existing StoryGenerator and CharacterThink spans MUST continue to report Summary bytes/tokens and Recent Story segment count/bytes/tokens without logging prose bodies.
- Prompt trace payloads MAY contain the composed RC as allowed by existing trace configuration, but the Story Continuity portion MUST contain no synthetic `sequence`, `text`, bullet, or quote wrapper.
- Pack validation errors MUST retain structured validation code and JSON path fields; no new metric is required.

---

## 5. Acceptance Criteria

- [ ] Story Pack v5 types match §3.1, and runtime/Prompt/API/UI code contains no Premise field or access — `rg -n 'pub premise:|\.premise\b|field\("premise"|format!\("premise:' crates/aise/src crates/aise-server/src crates/aise-server/assets examples config` returns zero matches; the explicit importer rejection check and negative test are allowed to contain the JSON key string.
- [ ] `StorySpec` exposes only `V5`/`aise_story_v5`, while `AssetSpecVersion` includes `V5_0` — verified by `story_pack_v5_discriminator_roundtrip`.
- [ ] A valid v5 Pack without Premise imports and exports successfully — verified by `story_pack_v5_import_export_roundtrip`.
- [ ] A v4 Pack is rejected as `UnsupportedSpec` at `/spec` — verified by `rejects_story_pack_v4`.
- [ ] A v5 Pack paired with `4.0` is rejected as `UnsupportedSpecVersion` at `/spec_version` — verified by `rejects_crossed_v5_spec_and_version`.
- [ ] A v5 Pack containing `/story/premise` is rejected as `SchemaInvalid` with message `premise is not supported` — verified by `rejects_removed_story_premise`.
- [ ] `RecentStoryPromptView` is deleted — `rg -n 'RecentStoryPromptView' crates/aise/src crates/aise/tests` returns zero matches.
- [ ] WriterPlanner renders `summary-one` exactly and `recent-one\n\nrecent-two` exactly, with no sequence, labels, bullets, or JSON quotes — verified by `writer_planner_renders_story_continuity_as_prose`.
- [ ] CharacterThink applies the identical rendering contract — verified by `character_think_renders_story_continuity_as_prose`.
- [ ] StoryGenerator applies the identical rendering contract — verified by `story_generator_renders_story_continuity_as_prose`.
- [ ] StoryRepairer preserves the StoryGenerator continuity bytes — verified by `story_repairer_reuses_story_continuity_prose`.
- [ ] All four RC templates render both subsections when populated and omit each empty subsection; both-empty input omits Story Continuity — verified by `story_continuity_template_omits_empty_sections` for every Prompt Profile.
- [ ] Story prose containing `{{ output_schema }}` or instruction-like Markdown remains literal RC data and never appears in CSI/FTI — verified by `story_continuity_is_not_recursively_rendered_or_promoted`.
- [ ] Internal ordering and boundary invariants remain intact — `cargo test -p aise story_continuity` passes without deleting Sequence or `summarized_through` assertions.
- [ ] Migration `0019_story_context_simplification.sql` matches §3.7, succeeds on an empty upgraded database, and rejects legacy rows with the named constraint — verified by `story_context_simplification_migration_guard`.
- [ ] `StoryView` serializes with no `premise` or `current_scene` property — verified by `story_api_omits_removed_context_fields`.
- [ ] The built-in Story Pack UI contains no Premise row or property access while retaining description, initial scene, and Opening — verified by `rg -n '\bpremise\b|前提' crates/aise-server/assets` returning zero matches and the existing UI smoke test.
- [ ] `examples/demo_pack.json` and every Story Pack fixture use the v5 pair and omit Premise; nested Character Card and World Book versions remain unchanged — verified by asset import tests.
- [ ] Existing prompt slot validation, prompt composition, engine flow, persistence, Story API, and SSE tests pass — `cargo test --workspace` passes.
- [ ] Formatting and linting pass — `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- A dedicated Story Summary generation pipeline may later define prose quality and compaction prompts; it must preserve the rendering contract in this spec.
- Any future user-facing synopsis field belongs to Pack metadata and requires its own asset design; it must not be injected as per-Turn current narrative state.
- Removing or simplifying static `StoryStart` metadata requires a separate Story Pack design.

---

## 7. References

- Source design: [Story Context Simplification](../design/2026-08-17-story-context-simplification-design-gpt.md)
- Required predecessor: [Current Scene Removal Spec](2026-08-17-current-scene-removal-spec-gpt.md)
- Continuity design: [Context Preparation and Retrieval](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Prompt architecture: [CSI-RC-FTI Prompt Framework](CSI-RC-FTI/2026-08-11-csi-rc-fti-prompt-spec-gpt.md)
- Guardrails: [`doc/agents/guardrails/`](../agents/guardrails/)
