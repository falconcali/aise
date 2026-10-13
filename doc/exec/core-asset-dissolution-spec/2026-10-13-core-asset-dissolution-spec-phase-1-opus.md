# Core Asset Dissolution Phase 1: Relocate Asset Types into Story and Role — Spec

> **Model**: Opus
> **Date**: 2026-10-13
> **Status**: Proposed
> **Source Design**: [aise-core 拆除 asset 模块 — Design](../../design/2026-10-13-core-asset-dissolution-design-opus.md)
> **Phase**: Phase 1 (of 2); depends on [Phase 0](2026-10-13-core-asset-dissolution-spec-phase-0-opus.md)

---

## 1. Goal

Delete `crates/aise-core/src/core/asset/` by moving story-pack types into a new `core/story/` directory module and character-card types into `core/role/`, replacing `AssetError` with `StoryError` and `EngineError::Asset` with typed `EngineError::Story` / `EngineError::Core`, extracting the shared digest into `core/digest.rs`, and splitting `core/story.rs` into `story_context.rs`, `story_instance.rs`, and `story_commit.rs`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Create `core/story/` (`mod.rs`, `story_pack.rs`, `story_context.rs`, `story_instance.rs`, `story_commit.rs`, `error.rs`, `tests/story_pack_tests.rs`) and delete `core/story.rs`.
- Create `core/role/character_card.rs` holding `CharacterCard`, `CharacterCardSpec`, `CharacterMeta`, `CharacterCardRef`, and `card_ref()`; remove `CharacterCardRef` from `core/role/actor.rs`.
- Create `core/digest.rs` with `Sha256Digest::canonical`.
- Add `CoreError::DigestFailed`.
- Replace `AssetError` with `StoryError`, which carries only story business errors.
- Replace `EngineError::Asset { message }` with `EngineError::Story(StoryError)` and `EngineError::Core(CoreError)`.
- Delete `core/asset/` entirely, including `tests/`.
- Rename `core/tests/asset_fixture.rs` to `core/tests/fixture.rs`.
- Update `core/mod.rs` re-exports per §3.6.
- Move and adapt tests per §3.7.

### 2.2 Non-Goals

- Does not change any field, serde attribute, or JSON shape of `StoryPack`, `CharacterCard`, `PackRef`, `StoryAssetSpec`, `CharacterCardRef`, `StoryContext`, `StoryInstanceSpec`, `StoryInstanceInfo`, `StoryLifeCycle`, `StorySummary`, or `StoryCommit`.
- Does not change the digest algorithm or output; existing `PackRef` / `CharacterCardRef` values stay valid.
- Does not change `StoryPack::validate()` or `StoryPack::resolve_cast()` logic (finalized in Phase 0).
- Does not change `StoryContext::new()`.
- Does not rename `StoryAssetSpec`, `PackRef`, or any other public type.
- Does not change `PackStore`, `StoryStore`, pipelines, prompts, or `aise-service` beyond what compilation requires.
- Does not add any boundary limit or config.
- Does not touch the `// WAIVER: R-ARCH-03` / `// WAIVER: R-ARCH-04` markers added in Phase 0.
- Does not change any `EngineError` variant other than `Asset`.
- Does not touch `crates/aise` or `crates/aise-server`.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, re-export aliases, or dual paths.
- `AssetError`, `core/asset/`, `core/story.rs`, `core/tests/asset_fixture.rs`, and `EngineError::Asset` MUST be deleted, not deprecated or aliased.
- Follow `AGENTS.md`: no comments other than the existing `// WAIVER:` markers (`R-CODE-05`), `mod.rs` index only and directory module layout (`R-CODE-01`), tests in `tests/<source>_tests.rs` (`R-CODE-02`), one contiguous `use` block (`R-CODE-07`).
- Inside `core/role/`, no file may reference any type defined in `core/story/`.

---

## 3. Contracts

### 3.1 `core/digest.rs`

```rust
impl Sha256Digest {
    pub fn canonical<T: Serialize>(subject: &'static str, value: &T) -> Result<Self, CoreError>;
}
```

Body is the current `canonical_digest` in `core/asset/document.rs`: `serde_json::to_vec(value)`, lowercase hex of `Sha256::digest`, wrapped with `Sha256Digest::try_new`. Both failure points map to `CoreError::DigestFailed { subject, message: error.to_string() }`.

### 3.2 `CoreError` (`crates/aise-core/src/core/error.rs`)

Add one variant; existing variants unchanged:

```rust
#[error("{subject} digest failed: {message}")]
DigestFailed { subject: &'static str, message: String },
```

### 3.3 `StoryError` (`crates/aise-core/src/core/story/error.rs`)

```rust
#[derive(Debug, Error)]
pub enum StoryError {
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

`StoryError` has no `CoreError` variant and no `From<CoreError>` impl; digest failures never pass through it.

### 3.4 Asset methods

```rust
impl StoryPack {
    pub fn validate(&self) -> Result<(), StoryError>;
    pub fn resolve_cast(
        &self,
        player_role: &RoleId,
        cast: BTreeMap<RoleId, RoleActor>,
    ) -> Result<BTreeMap<RoleId, RoleActor>, StoryError>;
    pub fn pack_ref(&self) -> Result<PackRef, CoreError>;
}

impl CharacterCard {
    pub fn card_ref(&self) -> Result<CharacterCardRef, CoreError>;
}
```

- `pack_ref()` uses `Sha256Digest::canonical("story_pack", self)`; `card_ref()` uses `Sha256Digest::canonical("character_card", self)`. The subject strings are private `const`s in `story_pack.rs` and `character_card.rs`.
- `StoryPack::digest()` and `CharacterCard::digest()` are deleted; callers read `pack_ref()?.digest` / `card_ref()?.digest`.

### 3.5 Engine error (`crates/aise-core/src/engine/error.rs`)

```rust
#[error("story error: {0}")]
Story(#[from] StoryError),
#[error("core error: {0}")]
Core(#[from] CoreError),
```

`Asset { message }` and `impl From<AssetError> for EngineError` are deleted. All other `EngineError` variants and their `From` impls are unchanged. In `AiseFactory`, story and digest errors propagate with plain `?`, with no `map_err`:

```rust
spec.story_pack.validate()?;
let pack_ref = spec.story_pack.pack_ref()?;
let card_ref = card.card_ref()?;
```

### 3.6 Module index files

`crates/aise-core/src/core/story/mod.rs`:

```rust
mod error;
mod story_commit;
mod story_context;
mod story_instance;
mod story_pack;

pub use error::StoryError;
pub use story_commit::StoryCommit;
pub use story_context::{StoryContext, StorySummary};
pub use story_instance::{StoryInstanceInfo, StoryInstanceSpec, StoryLifeCycle};
pub use story_pack::{
    PackRef, PlayDefinition, StoryAssetSpec, StoryPack, StoryPackMeta, StoryPackProfile, StoryPackSpec,
};
```

`crates/aise-core/src/core/role/mod.rs`:

```rust
mod actor;
mod character;
mod character_card;
mod memory;
mod role;

pub use actor::RoleActor;
pub use character::{CharacterProfile, DialogueExample};
pub use character_card::{CharacterCard, CharacterCardRef, CharacterCardSpec, CharacterMeta};
pub use memory::{MemoryBody, MemoryEntry};
pub use role::{RoleBody, RoleEntry};
```

`crates/aise-core/src/core/mod.rs` changes (all other lines unchanged):

```rust
mod digest;

pub use role::{
    CharacterCard, CharacterCardRef, CharacterCardSpec, CharacterMeta, CharacterProfile, DialogueExample, MemoryBody,
    MemoryEntry, RoleActor, RoleBody, RoleEntry,
};
pub use story::{
    PackRef, PlayDefinition, StoryAssetSpec, StoryCommit, StoryContext, StoryError, StoryInstanceInfo,
    StoryInstanceSpec, StoryLifeCycle, StoryPack, StoryPackMeta, StoryPackProfile, StoryPackSpec, StorySummary,
};
```

Removed from `core/mod.rs`: `mod asset;` and the whole `pub use asset::{...}` block.

### 3.7 File / Directory Layout

```text
crates/aise-core/src/core/
├── asset/                          DELETE (whole directory)
├── story.rs                        DELETE
├── story/
│   ├── mod.rs                      per §3.6
│   ├── error.rs                    StoryError per §3.3
│   ├── story_pack.rs               StoryPack, StoryPackSpec, StoryPackMeta, StoryPackProfile, PlayDefinition,
│   │                               PackRef, StoryAssetSpec; validate(), resolve_cast(), pack_ref(); private validate_play()
│   ├── story_context.rs            StoryContext (+ impl new()), StorySummary
│   ├── story_instance.rs           StoryInstanceSpec, StoryInstanceInfo, StoryLifeCycle
│   ├── story_commit.rs             StoryCommit
│   └── tests/story_pack_tests.rs
├── role/
│   ├── mod.rs                      per §3.6
│   ├── actor.rs                    RoleActor only
│   ├── character_card.rs           CharacterCard, CharacterCardSpec, CharacterMeta, CharacterCardRef; card_ref()
│   └── tests/character_card_tests.rs
├── digest.rs                       per §3.1
├── error.rs                        per §3.2
├── mod.rs                          per §3.6
└── tests/
    ├── mod.rs                      `mod fixture;` + same pub(crate) re-exports as today
    ├── asset_fixture.rs            DELETE
    └── fixture.rs                  content of asset_fixture.rs (post Phase 0), unchanged otherwise
crates/aise-core/src/engine/
├── error.rs                        per §3.5
├── factory.rs                      per §3.5
└── tests/factory_tests.rs          EngineError::Asset -> EngineError::Story
```

Test moves:

| Source (post Phase 0) | Target | Change |
|---|---|---|
| `core/asset/tests/story_pack_tests.rs` | `core/story/tests/story_pack_tests.rs` | `AssetError` -> `StoryError` |
| `core/asset/tests/document_tests.rs`: `example_story_packs_import`, `legacy_v5_pack_is_rejected_as_malformed` | `core/story/tests/story_pack_tests.rs` | unchanged logic |
| `core/asset/tests/document_tests.rs`: `digest_is_stable_hex_and_content_sensitive` | `core/story/tests/story_pack_tests.rs` | compare `pack_ref()?.digest` instead of `digest()` |
| `core/asset/tests/document_tests.rs`: `pack_ref_pins_pack_id_version_and_digest` | `core/story/tests/story_pack_tests.rs` | assert `pack_id`, `version`, and that `digest` equals a second `pack_ref()` call |
| `core/asset/tests/document_tests.rs`: `example_character_card_imports`, `card_ref_pins_card_id_version_and_digest` | `core/role/tests/character_card_tests.rs` | assert `card_id`, `version`, and that `digest` is 64 lowercase hex chars and equals a second `card_ref()` call |
| `engine/tests/factory_tests.rs` | same | `non_playable_player_role_is_rejected_without_retaining_assets` asserts `EngineError::Story(StoryError::RoleNotPlayable { .. })` instead of `EngineError::Asset { .. }` |

`story_pack.rs` declares `#[cfg(test)] #[path = "tests/story_pack_tests.rs"] mod tests;`; `character_card.rs` declares `#[cfg(test)] #[path = "tests/character_card_tests.rs"] mod tests;`.

---

## 4. Behavior Rules

1. **R-1**: Every public type listed in §3.6 keeps its exact field list, derives, and serde attributes from the pre-move code.
2. **R-2**: `crate::core::<Type>` paths used by `engine`, `persistence`, `pipeline`, and `aise-service` resolve unchanged for every type re-exported in §3.6; only `AssetError` (deleted) and `StoryError` (new) differ.
3. **R-3**: `pack_ref()` / `card_ref()` return byte-identical digests to Phase 0 for the same input (`serde_json::to_vec` + SHA-256 lowercase hex).
4. **R-4**: `StoryPack::validate()` and `resolve_cast()` return the same variants under the same conditions as Phase 0, now typed as `StoryError`.
5. **R-5**: Files under `core/role/` import nothing from `core/story/` (directly or via `crate::core::` re-exports of story types).
6. **R-6**: `core/story/*.rs` and `core/role/*.rs` import sibling-module items via `crate::core::...` paths; `super::` imports are used only in test files.
7. **R-7**: `core/digest.rs` contains only the `impl Sha256Digest` block and its imports.
8. **R-8**: The `// WAIVER: R-ARCH-03` and `// WAIVER: R-ARCH-04` lines in `AiseFactory::freeze_story` stay immediately above `spec.story_pack.validate()?;`.

### 4.1 Error Handling

- Story invariant violations surface as `EngineError::Story(StoryError)`, preserving the original variant (e.g. `StoryError::RoleNotPlayable { role_id }`).
- Digest failures surface as `EngineError::Core(CoreError::DigestFailed { subject, message })`.

### 4.2 Concurrency

- No change.

### 4.3 Observability

- No change.

---

## 5. Acceptance Criteria

- [ ] `crates/aise-core/src/core/asset/`, `crates/aise-core/src/core/story.rs`, `crates/aise-core/src/core/tests/asset_fixture.rs` do not exist
- [ ] `crates/aise-core/src/core/story/mod.rs` and `crates/aise-core/src/core/role/mod.rs` match §3.6 exactly
- [ ] `rg "AssetError|core::asset|asset_fixture|EngineError::Asset|canonical_digest" crates/aise-core crates/aise-service` returns zero matches
- [ ] `rg "StoryError::Core|map_err\(StoryError::from\)" crates/aise-core` returns zero matches
- [ ] `rg -n "// WAIVER: R-ARCH-0[34]" crates/aise-core crates/aise-service` returns exactly two matches, both in `crates/aise-core/src/engine/factory.rs` directly above `spec.story_pack.validate()?;`
- [ ] `rg "fn digest\(" crates/aise-core/src/core` returns zero matches
- [ ] `rg "Story(Pack|Error|Context|AssetSpec|Commit|Instance)|PackRef|PlayDefinition" crates/aise-core/src/core/role` returns zero matches
- [ ] `rg "^\s*(pub(\(crate\))?\s+)?(fn|struct|enum|impl|const|static|type) " crates/aise-core/src/core/story/mod.rs crates/aise-core/src/core/role/mod.rs crates/aise-core/src/core/mod.rs` returns zero matches
- [ ] `StoryError` matches §3.3, `CoreError::DigestFailed` matches §3.2, `EngineError::Story` and `EngineError::Core` match §3.5
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings` passes
- [ ] `cargo test -p aise-core` passes, including `core::story::story_pack::tests` and `core::role::character_card::tests`
- [ ] `cargo build -p aise-service` passes

---

## 6. Out of Scope / Future Work

- Unified boundary-condition design for asset input, Turn context, history, and memory → separate design doc (TBD)

---

## 7. References

- Source design: [2026-10-13-core-asset-dissolution-design-opus.md](../../design/2026-10-13-core-asset-dissolution-design-opus.md)
- Previous phase: [Phase 0](2026-10-13-core-asset-dissolution-spec-phase-0-opus.md)
- Guardrails: `AGENTS.md`, `doc/agents/guardrails/code-organization.md`
