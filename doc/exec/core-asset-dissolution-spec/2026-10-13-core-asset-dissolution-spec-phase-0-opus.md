# Core Asset Dissolution Phase 0: Remove Boundary Config and Document Parsing — Spec

> **Model**: Opus
> **Date**: 2026-10-13
> **Status**: Proposed
> **Source Design**: [aise-core 拆除 asset 模块 — Design](../../design/2026-10-13-core-asset-dissolution-design-opus.md)
> **Phase**: Phase 0 (of 2)

---

## 1. Goal

Delete `AssetConfig`, `NarrativeConfig`, all asset/narrative size-limit validation, and `from_json` document parsing from `aise-core`, keeping only the structural business invariants (`StoryPack::validate()`, `StoryPack::resolve_cast()`, `NarrativeGraph::validate()`) and asset digests, without moving any type out of `core/asset/`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete `crates/aise-core/src/core/asset/config.rs` (`AssetConfig`).
- Delete `crates/aise-core/src/core/narrative/config.rs` (`NarrativeConfig`).
- Delete `crates/aise-core/src/core/asset/validate.rs`; move the surviving structural check into `StoryPack::validate()` in `core/asset/story_pack.rs`.
- Delete `StoryPack::from_json`, `CharacterCard::from_json`, and `parse_document` from `core/asset/document.rs`.
- Delete `CharacterCard::validate`.
- Remove the limit-related variants from `AssetError` and `NarrativeError`.
- Remove the config parameter from `NarrativeGraph::validate` and the depth parameter from `NarrativeCondition::validate`.
- Remove `asset_config` from `EngineConfig`, `AiseEngine`, and `AiseFactory`.
- Mark the `R-ARCH-03` / `R-ARCH-04` waiver boundary in `AiseFactory::freeze_story` (§3.4).
- In `aise-service`, replace `from_json` with `serde_json::from_str` and add the `serde_json` dependency.
- Update or delete the affected tests as listed in §3.6.

### 2.2 Non-Goals

- Does not move `StoryPack`, `CharacterCard`, `PackRef`, `StoryAssetSpec`, or `CharacterCardRef` to another module (Phase 1).
- Does not rename `AssetError` or `EngineError::Asset` (Phase 1).
- Does not introduce `core/digest.rs`, `StoryError`, or `CoreError::DigestFailed` (Phase 1).
- Does not split `core/story.rs` (Phase 1).
- Does not rename `core/tests/asset_fixture.rs` (Phase 1).
- Does not change `BoundedText`, `RetrievalHint`, or any ID type.
- Does not change the JSON shape of story packs or character cards; `examples/*.json` stay untouched.
- Does not add any replacement limit, cap, or config for asset input.
- Does not touch `crates/aise`, `crates/aise-server`, or any pipeline module.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, or dual-write logic.
- Old types / functions / modules superseded by this spec MUST be deleted, not deprecated.
- No `#[allow(dead_code)]` to keep removed helpers alive.
- Follow `AGENTS.md`: no comments except the two `// WAIVER:` lines in §3.4 (`R-CODE-05`), `mod.rs` index only (`R-CODE-01`), tests in `tests/<source>_tests.rs` (`R-CODE-02`), one contiguous `use` block (`R-CODE-07`).

---

## 3. Contracts

### 3.1 `AssetError` (`crates/aise-core/src/core/asset/error.rs`)

Final shape. Error messages of the retained variants stay byte-identical to the current code.

```rust
#[derive(Debug, Error)]
pub enum AssetError {
    #[error("{asset} digest failed: {message}")]
    DigestFailed { asset: &'static str, message: String },

    #[error("story pack defines no playable role")]
    NoPlayableRole,

    #[error("{field} references unknown role: {role_id}")]
    UnknownRole { field: &'static str, role_id: String },

    #[error("role is not playable: {role_id}")]
    RoleNotPlayable { role_id: String },

    #[error("narrative graph is invalid: {0}")]
    Narrative(#[from] NarrativeError),
}
```

Deleted variants: `DocumentTooLarge`, `MalformedDocument`, `TextTooLong`, `TooManyItems`, `InvalidEntry`.

### 3.2 `NarrativeError` (`crates/aise-core/src/core/narrative/error.rs`)

```rust
#[derive(Debug, Error)]
pub enum NarrativeError {
    #[error("narrative {operator} condition must not be empty")]
    EmptyCondition { operator: &'static str },

    #[error("narrative condition references unknown node: {node_id}")]
    UnknownNode { node_id: String },

    #[error("semantic condition {condition_id} is declared with different criteria")]
    SemanticCriterionConflict { condition_id: String },
}
```

Deleted variants: `TooManyNodes`, `ConditionTooDeep`, `TooManyConstraints`, `TooManySemanticConditions`.

### 3.3 Functions

```rust
impl NarrativeGraph {
    pub fn validate(&self) -> Result<(), NarrativeError>;
}

impl NarrativeCondition {
    pub(super) fn validate(
        &self,
        nodes: &BTreeMap<NarrativeNodeId, NarrativeNode>,
    ) -> Result<(), NarrativeError>;
}

impl StoryPack {
    pub fn validate(&self) -> Result<(), AssetError>;
    pub fn resolve_cast(
        &self,
        player_role: &RoleId,
        cast: BTreeMap<RoleId, RoleActor>,
    ) -> Result<BTreeMap<RoleId, RoleActor>, AssetError>;
    pub fn digest(&self) -> Result<Sha256Digest, AssetError>;
    pub fn pack_ref(&self) -> Result<PackRef, AssetError>;
}

impl CharacterCard {
    pub fn digest(&self) -> Result<Sha256Digest, AssetError>;
    pub fn card_ref(&self) -> Result<CharacterCardRef, AssetError>;
}

impl AiseFactory {
    pub async fn create_story(
        &self,
        store: Arc<Store>,
        spec: StoryCreationSpec,
    ) -> Result<StoryInstanceInfo, EngineError>;
    fn freeze_story(&self, story_id: StoryId, spec: StoryCreationSpec) -> Result<FrozenStory, EngineError>;
    fn freeze_cast(&self, cast: BTreeMap<RoleId, RoleActorSpec>) -> Result<FrozenCast, EngineError>;
}
```

Deleted functions: `StoryPack::from_json`, `CharacterCard::from_json`, `CharacterCard::validate`, `parse_document`, and every private helper in `validate.rs` (`validate_pack_meta`, `validate_card_meta`, `validate_pack_profile`, `validate_role`, `validate_profile`, `validate_narrative`, `validate_narrative_node`, `validate_recallable`, `validate_entries`, `check_text`, `check_optional_text`, `check_list`, `check_count`). `NarrativeCondition::validate_at` is folded into `NarrativeCondition::validate`.

### 3.4 Engine config, state, and waiver marker

```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub llm_config: LlmConfig,
    pub prompt_config: PromptConfig,
    pub persistence_config: PersistanceConfig,
    pub pipeline_config: PipelineConfig,
}

pub struct AiseEngine {
    runtime: Runtime,
    store: Arc<Store>,
}
```

`AiseFactory::freeze_story` marks the waiver boundary on the line that admits the story pack into core. These are the only two comment lines added by this phase:

```rust
// WAIVER: R-ARCH-03
// WAIVER: R-ARCH-04
spec.story_pack.validate()?;
```

Removing the limits leaves `opening` and `constraints` prompt size, `PackStore` snapshot size, and the per-Turn semantic query count without an upper bound; see the source design §3 for the full list. The markers are removed by the future boundary-condition design, not by Phase 1.

### 3.5 `aise-service`

`crates/aise-service/Cargo.toml` adds:

```toml
serde_json.workspace = true
```

`create_demo_story` in `crates/aise-service/src/main.rs` parses with `serde_json`:

```rust
let story_pack: StoryPack = serde_json::from_str(&read_demo_asset(DEMO_STORY_PACK_PATH).await?)
    .context("demo story pack parse failed")?;
let card: CharacterCard = serde_json::from_str(&read_demo_asset(DEMO_CHARACTER_CARD_PATH).await?)
    .context("demo character card parse failed")?;
```

### 3.6 File / Directory Layout

```text
crates/aise-core/src/core/
├── asset/
│   ├── mod.rs                    remove `mod config;`, `mod validate;`, `pub use config::AssetConfig;`
│   ├── character_card.rs         unchanged
│   ├── config.rs                 DELETE
│   ├── document.rs               keep digest(), pack_ref(), card_ref(), canonical_digest(); drop from_json/parse_document
│   ├── error.rs                  per §3.1
│   ├── story_pack.rs             add StoryPack::validate() and private validate_play()
│   ├── validate.rs               DELETE
│   └── tests/
│       ├── document_tests.rs     see test table
│       ├── story_pack_tests.rs   see test table
│       └── validate_tests.rs     DELETE
├── narrative/
│   ├── mod.rs                    remove `mod config;`, `pub use config::NarrativeConfig;`
│   ├── config.rs                 DELETE
│   ├── condition.rs              validate() without depth
│   ├── error.rs                  per §3.2
│   ├── narrative.rs              validate() without config
│   └── tests/narrative_tests.rs  see test table
├── mod.rs                        drop `AssetConfig` and `NarrativeConfig` from re-exports
└── tests/asset_fixture.rs        use serde_json::from_str
crates/aise-core/src/engine/
├── config.rs                     per §3.4
├── engine.rs                     per §3.4; create_story no longer passes asset_config
├── factory.rs                    per §3.3; waiver markers per §3.4
└── tests/factory_tests.rs        see test table
crates/aise-service/
├── Cargo.toml                    per §3.5
└── src/main.rs                   per §3.5
```

Test changes:

| File | Change |
|---|---|
| `core/asset/tests/validate_tests.rs` | DELETE. Move `valid_pack_passes_validation`, `empty_playable_roles_are_rejected`, `playable_role_must_be_defined`, `pack_with_valid_narrative_passes_validation`, `pack_with_invalid_narrative_is_rejected` into `core/asset/tests/story_pack_tests.rs`, calling `validate()` with no argument and reusing that file's `pack_json()`. Drop `opening_longer_than_limit_is_rejected`, `seed_memory_error_reports_role_and_memory_ids`, `role_backstory_error_names_backstory_field`, `too_many_seed_facts_are_rejected`, `character_card_name_longer_than_limit_is_rejected`, `narrative_constraint_longer_than_text_limit_is_rejected`. |
| `core/asset/tests/document_tests.rs` | Keep `example_story_packs_import`, `example_character_card_imports`, `pack_ref_pins_pack_id_version_and_digest`, `card_ref_pins_card_id_version_and_digest`. Rewrite `legacy_v5_pack_is_rejected_as_malformed` as `serde_json::from_str::<StoryPack>(LEGACY_V5_PACK_JSON).is_err()`. Rewrite `digest_is_stable_hex_and_content_sensitive` to re-parse with `serde_json::from_str`. Delete `invalid_json_is_rejected_as_malformed`, `oversized_document_is_rejected_before_parsing`, `imported_document_is_validated_against_limits`. |
| `core/narrative/tests/narrative_tests.rs` | Delete `condition_deeper_than_limit_is_rejected`, `too_many_nodes_are_rejected`, `too_many_semantic_conditions_are_rejected`, `too_many_constraints_on_a_node_are_rejected`. Remaining tests call `graph.validate()`. |
| `engine/tests/factory_tests.rs` | `create_story` helper drops the `asset_config` parameter. Delete `story_pack_violating_asset_limits_is_rejected` and `character_card_violating_asset_limits_is_rejected`. Remove now-unused imports (`BoundedText`). |
| `core/tests/asset_fixture.rs` | `snake_pack()`, `demo_pack()`, `xu_xian_card()` use `serde_json::from_str(...).expect(...)`; drop the `AssetConfig` import. |

---

## 4. Behavior Rules

1. **R-1**: `StoryPack::validate()` returns `Err(AssetError::NoPlayableRole)` when `play.playable_roles` is empty.
2. **R-2**: `StoryPack::validate()` returns `Err(AssetError::UnknownRole { field: "play.playable_roles", role_id })` for the first role in `play.playable_roles` (iteration order of `BTreeSet`) not present in `roles`.
3. **R-3**: When R-1 and R-2 pass and `narrative` is `Some(graph)`, `StoryPack::validate()` returns `graph.validate()` mapped through `AssetError::Narrative`. When `narrative` is `None`, it returns `Ok(())`.
4. **R-4**: `StoryPack::validate()` performs no length, count, or byte-size checks on any field.
5. **R-5**: `NarrativeGraph::validate()` iterates every condition of every node (`activate_when`, `complete_when`, `skip_when`) and, for each, runs `NarrativeCondition::validate(&self.nodes)` then `collect_semantic`, returning the first error.
6. **R-6**: `NarrativeCondition::validate` returns `EmptyCondition { operator: "all" | "any" }` for an empty `All`/`Any`, `UnknownNode` for a `NodeIs` referencing a node not in `nodes`, recurses into children of `All`/`Any`/`Not`, and returns `Ok(())` otherwise. It has no depth limit.
7. **R-7**: `StoryPack::resolve_cast()` behavior is unchanged.
8. **R-8**: `StoryPack::digest()` / `CharacterCard::digest()` produce the same lowercase hex SHA-256 over `serde_json::to_vec(self)` as before; existing `PackRef` / `CharacterCardRef` values remain valid.
9. **R-9**: `AiseFactory::freeze_story` calls `spec.story_pack.validate()` first, then `freeze_cast`, then `resolve_cast`, then `pack_ref`, preserving the current ordering. The `validate()` line is immediately preceded by `// WAIVER: R-ARCH-03` and `// WAIVER: R-ARCH-04`, in that order.
10. **R-10**: `AiseFactory::freeze_cast` maps `RoleActorSpec::Card(card)` to `RoleActor::Card(card.card_ref()?)` and pushes `(card_ref, *card)` into `FrozenCast.cards`; it performs no card validation.
11. **R-11**: The asset-release-on-failure behavior of `AiseFactory::create_story` is unchanged.

### 4.1 Error Handling

- `AssetError` still converts to `EngineError::Asset { message }` via the existing `From<AssetError>` impl.
- `aise-service` reports parse failures through `anyhow::Context` with the messages in §3.5.

### 4.2 Concurrency

- No change.

### 4.3 Observability

- No change. The existing `tracing::error!` in `release_story_assets` stays as is.

---

## 5. Acceptance Criteria

- [ ] `rg "AssetConfig|NarrativeConfig|asset_config" crates/aise-core crates/aise-service` returns zero matches
- [ ] `rg "from_json|parse_document" crates/aise-core crates/aise-service` returns zero matches
- [ ] `rg "DocumentTooLarge|MalformedDocument|TextTooLong|TooManyItems|InvalidEntry" crates/aise-core` returns zero matches
- [ ] `rg "TooManyNodes|ConditionTooDeep|TooManyConstraints|TooManySemanticConditions|max_depth" crates/aise-core` returns zero matches
- [ ] `crates/aise-core/src/core/asset/config.rs`, `crates/aise-core/src/core/asset/validate.rs`, `crates/aise-core/src/core/asset/tests/validate_tests.rs`, `crates/aise-core/src/core/narrative/config.rs` do not exist
- [ ] `AssetError` matches §3.1 and `NarrativeError` matches §3.2
- [ ] `EngineConfig` and `AiseEngine` match §3.4
- [ ] `rg -n "// WAIVER: R-ARCH-0[34]" crates/aise-core crates/aise-service` returns exactly two matches, both in `crates/aise-core/src/engine/factory.rs` directly above `spec.story_pack.validate()?;`
- [ ] `crates/aise-service/Cargo.toml` contains `serde_json.workspace = true`
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings` passes
- [ ] `cargo test -p aise-core` passes, including the five validation tests moved into `story_pack_tests.rs`
- [ ] `cargo build -p aise-service` passes

---

## 6. Out of Scope / Future Work

- Relocating asset types into `core/story/` and `core/role/`, introducing `StoryError`, `core/digest.rs`, and `CoreError::DigestFailed`, deleting `core/asset/` → `doc/exec/core-asset-dissolution-spec/2026-10-13-core-asset-dissolution-spec-phase-1-opus.md`
- Unified boundary-condition design for asset input, Turn context, history, and memory → separate design doc (TBD)

---

## 7. References

- Source design: [2026-10-13-core-asset-dissolution-design-opus.md](../../design/2026-10-13-core-asset-dissolution-design-opus.md)
- Current code: `crates/aise-core/src/core/asset/`, `crates/aise-core/src/core/narrative/`, `crates/aise-core/src/engine/factory.rs`
- Guardrails: `AGENTS.md`, `doc/agents/guardrails/code-organization.md`
