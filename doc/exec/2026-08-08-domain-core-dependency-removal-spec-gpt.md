# Domain-to-Core Dependency Removal — Spec

> **Model**: GPT-5
> **Date**: 2026-08-08
> **Status**: Proposed
> **Source Design**: [AISE Technical Architecture v3.1](../design/2026-08-04-Architecture-gpt.md)
> **Supersedes**: [2026-08-06 Domain-to-Core Dependency Removal Spec](./2026-08-06-domain-core-dependency-removal-spec-gpt.md) in full
> **Related Contract**: [Story Pack v3 Spec](./2026-08-07-story-pack-v3-spec-gpt.md)
> **Reviewed Baseline**: `main@98d565ef4149fcdde8cad0ae60b44da2f249b8d7`
> **Phase**: N/A

---

## 1. Goal

Eliminate every `domain -> core` dependency by making Domain the sole owner of persistent Story identity, Turn identity, Story revision, and Story constraint identity while keeping Core-owned request and Trace errors scoped to their own contracts.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Move `StoryId`, `TurnId`, and `StoryRevision` from `core::turn_contract` into `domain::ids`.
- Replace both current `ConstraintId` definitions with one `domain::ids::ConstraintId`.
- Add `domain::error::DomainInputError` for Domain-owned ID validation.
- Replace `TurnInputError` with request-only `core::turn_contract::TurnRequestError`.
- Add `core::turn_trace::TraceIdError` and return it from `TraceId::try_new`.
- Delete the unused AISE library `SessionId`; keep only `aise-server::session::SessionId`.
- Make `TurnIdentity::new` infallible because all of its inputs are validated newtypes.
- Update all new Story Pack v3 Domain modules that currently import `core::turn_contract`.
- Update every affected production import, public re-export, constructor call, test import, and error mapping.
- Add an integration contract test and a CI boundary check that reject any future `domain -> core` backedge.

### 2.2 Non-Goals

- Does not merge `core` and `domain`.
- Does not relocate, rename, merge, or re-export either existing `StoryReadSnapshot` type:
  - `domain::story_state::StoryReadSnapshot` remains the legacy Turn snapshot used by the current Runtime and Store.
  - `domain::story_instance::snapshot::StoryReadSnapshot` remains the Story Pack v3 snapshot used by Narrative Graph code.
- Does not integrate the Story Pack v3 snapshot into `Store::load_story_snapshot` or delete the legacy Story state model.
- Does not merge the legacy and Story Pack v3 `CurrentScene` or `StoryConstraint` structures; only their `ConstraintId` field type is unified.
- Does not change `CharacterId`, `EventId`, `MemoryId`, `FactId`, asset key types, their infallible `From` implementations, or their validation rules.
- Does not move Domain invariant enforcement out of Validation.
- Does not change `StoryProposal`, `ValidationResult`, `ValidatedChangeSet`, Pipeline order, Validation/Repair behavior, Turn commit semantics, or Store transaction boundaries.
- Does not restore currently disabled Context Retrieval, Character Thinking, or deterministic Validation behavior.
- Does not change `TraceId::new_id`, `TraceId::file_stem`, Trace filename formatting, or Trace persistence behavior.
- Does not change database schema, SQL column types, HTTP routes, HTTP response shapes, SSE event shapes, JSON field names, or persisted ID/revision values.
- Does not remove the separate `core -> config` dependency.

### 2.3 Implementation Constraints

- Implement the final ownership model in one change. Do not retain fallback paths, deprecated aliases, compatibility re-exports, adapter types, duplicate definitions, or dual APIs (`R-REFACTOR-01`, `R-REFACTOR-02`).
- The final dependency direction is `core -> domain`; every Rust source under `crates/aise/src/domain/` must contain zero imports, re-exports, aliases, or qualified references to `core` (`R-ARCH-01`, `R-LAYER-01`).
- `mod.rs` and `lib.rs` remain index-only. Generated Rust code contains no ordinary comments. Tests use the crate integration-test directory or dedicated `tests/<source>_tests.rs` files (`R-CODE-01`, `R-CODE-02`, `R-CODE-05`).
- Core and Domain public errors remain typed `thiserror` errors and never expose `anyhow::Error` (`R-OBS-05`).
- Use the existing `serde`, `thiserror`, and `uuid` dependencies. Add no dependency.
- Preserve the existing valid wire form: string IDs remain JSON strings and `StoryRevision` remains a JSON integer.
- Preserve the input text exactly for every accepted ID. Use `trim()` only to decide whether the input is blank; do not trim a valid non-blank ID before storing it.
- Historical design, review, and execution documents remain unchanged. This file supersedes the 2026-08-06 version instead of editing historical content.

### 2.4 Required Implementation Order

1. Add `DomainInputError`; define `StoryId`, `TurnId`, `StoryRevision`, and `ConstraintId` in `domain::ids`.
2. Convert every Domain consumer to `domain::ids`, consolidate both `ConstraintId` definitions, and delete Domain-only dead anchors tied to Core imports.
3. Import Domain-owned types into `core::turn_contract`; add `TurnRequestError`; delete Core `SessionId`; make `TurnIdentity::new` infallible.
4. Add `TraceIdError`; update Core, Story, Persistence, Engine, Validation, Server tests, and all integration-test call sites.
5. Delete every obsolete definition, old import, old re-export, old error name, and obsolete result-handling branch.
6. Add the integration contract tests and CI boundary check, then run the complete workspace verification matrix.

No intermediate state in which old and new owners coexist may be committed.

---

## 3. Contracts

### 3.1 Final Dependency and Ownership Contract

The final compile-time dependency shape is:

```text
aise-server -> engine/runtime/pipelines/persistence -> core -> domain
                                                      |       ^
                                                      +-------+

domain -X-> core
domain -X-> runtime
domain -X-> persistence
domain -X-> aise-server
```

Type ownership is fixed as follows:

| Type | Sole definition | Canonical public path |
|---|---|---|
| `StoryId` | `crates/aise/src/domain/ids.rs` | `aise::domain::ids::StoryId` |
| `TurnId` | `crates/aise/src/domain/ids.rs` | `aise::domain::ids::TurnId` |
| `StoryRevision` | `crates/aise/src/domain/ids.rs` | `aise::domain::ids::StoryRevision` |
| `ConstraintId` | `crates/aise/src/domain/ids.rs` | `aise::domain::ids::ConstraintId` |
| `DomainInputError` | `crates/aise/src/domain/error.rs` | `aise::domain::error::DomainInputError` |
| `TurnRequestError` | `crates/aise/src/core/turn_contract.rs` | `aise::core::turn_contract::TurnRequestError` |
| `TraceIdError` | `crates/aise/src/core/turn_trace.rs` | `aise::core::turn_trace::TraceIdError` |
| Server `SessionId` | `crates/aise-server/src/session/model.rs` | `aise_server::session::SessionId` |

`domain/mod.rs` also re-exports `StoryId`, `TurnId`, `StoryRevision`, `ConstraintId`, and `DomainInputError`. Core must not re-export any Domain-owned ID or revision type.

### 3.2 Baseline Backedge Closure Matrix

Every row is mandatory for the reviewed baseline:

| Current file | Current Core dependency | Required final action |
|---|---|---|
| `domain/ids.rs` | Re-exports `SessionId`, `StoryId`, `TurnId` from Core | Define Domain-owned IDs/revision; delete `SessionId` re-export |
| `domain/story_state.rs` | Imports `StoryId`, `StoryRevision`; returns `TurnInputError` | Import IDs and `ConstraintId` from Domain; delete local `ConstraintId` |
| `domain/knowledge/fact.rs` | Imports `StoryRevision` | Import `domain::ids::StoryRevision` |
| `domain/knowledge/memory.rs` | Imports `StoryRevision` | Import `domain::ids::StoryRevision` |
| `domain/knowledge/rumor.rs` | Imports `StoryRevision` | Import `domain::ids::StoryRevision` |
| `domain/knowledge/query.rs` | Imports `StoryRevision`, `TurnId` | Import both from `domain::ids` |
| `domain/story_instance/binding.rs` | Imports `StoryId`, `StoryRevision` | Import both from `domain::ids` |
| `domain/story_instance/snapshot.rs` | Imports IDs/revision; returns `TurnInputError` | Import IDs and `ConstraintId` from Domain; delete local `ConstraintId` and `_snapshot_anchor` |
| `domain/narrative_graph/state.rs` | Imports `TurnId` | Import `domain::ids::TurnId` |
| `domain/narrative_graph/director.rs` | Imports `TurnId` only for a dead anchor | Delete `_director_anchor` and its anchor-only imports |

After these changes, no file under `crates/aise/src/domain/` may name `crate::core`, any ancestor `super::core`, or a grouped `crate::{core::...}` import.

### 3.3 Domain Input Error Contract

Create `crates/aise/src/domain/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainInputError {
    #[error("story_id must not be empty")]
    EmptyStoryId,
    #[error("turn_id must not be empty")]
    EmptyTurnId,
    #[error("constraint_id must not be empty")]
    EmptyConstraintId,
}
```

No Core, Store, Server, Trace, or asset-specific error variant may be added to `DomainInputError`.

### 3.4 Domain IDs and Revision Contract

`crates/aise/src/domain/ids.rs` must own these public types:

```rust
use crate::domain::error::DomainInputError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoryId(Arc<str>);

impl StoryId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TurnId(Arc<str>);

impl TurnId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError>;
    pub fn new_uuid() -> Self;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstraintId(Arc<str>);

impl ConstraintId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoryRevision(u64);

impl StoryRevision {
    pub const fn new(value: u64) -> Self;
    pub const fn get(&self) -> u64;
}
```

The required trait contract is:

| Type | Required traits/implementations |
|---|---|
| `StoryId` | `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Display`, manual `Serialize`, validated manual `Deserialize` |
| `TurnId` | `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Display`, manual `Serialize`, validated manual `Deserialize` |
| `ConstraintId` | `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Display`, manual `Serialize`, validated manual `Deserialize` |
| `StoryRevision` | Existing `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Display`, `Serialize`, `Deserialize` behavior |

For `StoryId`, `TurnId`, and `ConstraintId`:

- `Serialize` emits the inner value as a JSON string.
- `Deserialize` deserializes a string and delegates to `try_new`; it must not construct the private field directly.
- There is no public `From<String>`, `From<&str>`, `Default`, tuple-field visibility, unchecked constructor, or Serde path that can create a blank value.
- The existing private `id_type!` macro for `CharacterId`, `EventId`, `MemoryId`, and `FactId` must remain behaviorally unchanged. Implement the three validated IDs explicitly; do not add them to `id_type!` or another macro.

`domain/mod.rs` must expose the affected public surface as follows:

```rust
pub mod error;
pub mod ids;

pub use error::DomainInputError;
pub use ids::{
    CharacterId, ConstraintId, EventId, FactId, MemoryId, StoryId, StoryRevision, TurnId,
};
```

The abbreviated module declarations constrain only the affected entries. Existing unrelated Domain declarations and re-exports remain unchanged. `domain/mod.rs` must not export `SessionId`.

Remove `ConstraintId` from the existing `pub use story_state::{...}` list. The root `aise::domain::ConstraintId` path must resolve only through `pub use ids::ConstraintId`.

### 3.5 Shared Constraint ID Contract

Delete both current `ConstraintId` definitions from:

- `crates/aise/src/domain/story_state.rs`
- `crates/aise/src/domain/story_instance/snapshot.rs`

Both modules import the same Domain ID and retain their distinct `StoryConstraint` payload types:

```rust
use crate::domain::ids::ConstraintId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryConstraint {
    pub id: ConstraintId,
    pub text: String,
}
```

Neither old module may publicly re-export `ConstraintId`. All production and test callers migrate to `crate::domain::ids::ConstraintId` or `aise::domain::ConstraintId`.

### 3.6 Core Turn Request and Identity Contract

`crates/aise/src/core/turn_contract.rs` imports, but does not define or re-export, Domain-owned types:

```rust
use crate::domain::ids::{StoryId, StoryRevision, TurnId};
```

Replace `TurnInputError` with the request-only error:

```rust
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TurnRequestError {
    #[error("idempotency key must not be empty")]
    EmptyIdempotencyKey,
    #[error("idempotency key is {actual} chars, maximum {maximum}")]
    IdempotencyKeyTooLong { actual: usize, maximum: usize },
    #[error("player input must not be empty")]
    EmptyPlayerInput,
    #[error("player input is {actual} chars, maximum {maximum}")]
    PlayerInputTooLong { actual: usize, maximum: usize },
}
```

The affected public signatures become:

```rust
impl IdempotencyKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TurnRequestError>;
}

impl TurnRequest {
    pub fn try_new(player_input: String) -> Result<Self, TurnRequestError>;
}

impl ExecuteTurnSpec {
    pub fn try_into_validated(self) -> Result<ValidatedExecuteTurnSpec, TurnRequestError>;
}

impl TurnIdentity {
    pub fn new(
        story_id: StoryId,
        turn_id: TurnId,
        idempotency_key: IdempotencyKey,
        started_at_ms: i64,
    ) -> Self;
}
```

`TurnIdentity::new` performs no duplicate string validation and contains no failure branch. `CommittedTurnResult`, `ExecuteTurnSpec`, `ValidatedExecuteTurnSpec`, `TurnIdentity`, and all other Core contracts continue to use the Domain-owned types directly.

Delete the complete AISE library `SessionId` declaration and implementation from `core::turn_contract`. Do not replace it inside the `aise` crate.

`core/mod.rs` remains on its current minimal public surface. It must not add re-exports for `StoryId`, `TurnId`, `StoryRevision`, `ConstraintId`, `DomainInputError`, `TurnRequestError`, `TraceIdError`, or `SessionId`.

### 3.7 Trace ID Error Contract

`crates/aise/src/core/turn_trace.rs` owns the Trace-specific error:

```rust
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TraceIdError {
    #[error("trace_id must not be empty")]
    EmptyTraceId,
}

impl TraceId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TraceIdError>;
    pub fn new_id() -> Self;
    pub fn as_str(&self) -> &str;
    pub fn file_stem(&self) -> &str;
}
```

`TraceId::try_new` returns `TraceIdError::EmptyTraceId` for empty or whitespace-only input. It never returns a Story, Turn, Domain, request, or Session error. The current timestamp-plus-UUID generation and `file_stem` extraction remain unchanged.

### 3.8 Snapshot Placement Contract

This spec preserves both current snapshot types and removes the snapshot relocation requirement from the superseded 2026-08-06 spec.

| Snapshot | Required final location | Required ID imports |
|---|---|---|
| Legacy Runtime snapshot | `domain/story_state.rs` | `domain::ids::{StoryId, StoryRevision}` |
| Story Pack v3 snapshot | `domain/story_instance/snapshot.rs` | `domain::ids::{ConstraintId, StoryId, StoryRevision}` |

The v3 snapshot must delete its unused `TurnId` import and `_snapshot_anchor`. Constructors, private fields, accessors, collection order, clone behavior, and `base_revision` semantics of both snapshots remain unchanged.

### 3.9 Public Path Migration

| Old path/name | Required final path/action |
|---|---|
| `aise::core::turn_contract::StoryId` | `aise::domain::ids::StoryId` or `aise::domain::StoryId` |
| `aise::core::turn_contract::TurnId` | `aise::domain::ids::TurnId` or `aise::domain::TurnId` |
| `aise::core::turn_contract::StoryRevision` | `aise::domain::ids::StoryRevision` or `aise::domain::StoryRevision` |
| `crate::core::turn_contract::{StoryId, TurnId, StoryRevision}` | Import the corresponding `crate::domain::ids::*` types |
| `domain::story_state::ConstraintId` | `domain::ids::ConstraintId` or `domain::ConstraintId` |
| `domain::story_instance::snapshot::ConstraintId` | `domain::ids::ConstraintId` or `domain::ConstraintId` |
| `TurnInputError` | `DomainInputError`, `TurnRequestError`, or `TraceIdError` according to ownership |
| `aise::core::turn_contract::SessionId` | Delete; no replacement inside `aise` |
| `aise::domain::ids::SessionId` / `aise::domain::SessionId` | Delete; Server keeps `aise_server::session::SessionId` |

Do not preserve any old path through `pub use`, a type alias, a deprecated item, a wrapper, or an adapter.

### 3.10 Production Call-Site Contract

The following production changes are required in addition to Domain changes:

| File | Required change |
|---|---|
| `core/turn_event.rs` | Import `TurnId` from `domain::ids`; keep `CommittedTurnResult` in Core |
| `core/turn_trace.rs` | Use `TraceIdError`; keep existing Domain `StoryId`/`TurnId` imports |
| `story/instance_factory.rs` | Import `StoryId` and `StoryRevision` from `domain::ids` |
| `persistence/store.rs` | Import `StoryRevision` from `domain::ids`; keep Turn request/result types in Core |
| `persistence/sqlite_store.rs` | Import `StoryRevision` from `domain::ids`; map invalid persisted `TurnId` to serialization failure |
| `validation/validation_pipeline.rs` | Construct `domain::ids::ConstraintId`; retain the existing invariant-error conversion |
| `engine.rs` | Construct `TurnIdentity` directly; delete the impossible `Ok`/`Err` branch |
| `domain/narrative_graph/director.rs` | Delete `_director_anchor`; remove anchor-only `TurnId`, `NarrativeNodeDefinition`, and `ImpulseUrgency` imports |

All `TurnIdentity::new` test call sites remove `.unwrap()`, `.expect()`, and `match` handling applied to that constructor. Error handling for `IdempotencyKey::try_new`, `TurnRequest::try_new`, `StoryId::try_new`, `TurnId::try_new`, `ConstraintId::try_new`, and `TraceId::try_new` remains fallible.

### 3.11 Persistence and Wire Compatibility Contract

No SQL migration is created. The representation remains:

| Value | Persisted/JSON representation |
|---|---|
| `StoryId` | Original string |
| `TurnId` | Original string |
| `ConstraintId` | Original string |
| `StoryRevision` | Existing non-negative integer |

Persisted invalid Turn IDs are classified as invalid stored data, not Store availability failures:

```rust
let id = TurnId::try_new(id).map_err(|_| StoreError::Serialization {
    kind: StoreSerializationErrorKind::InvalidTurnResult,
})?;
```

Serde failures caused by a blank Domain ID are mapped at the adapter boundary into the existing `StoreError::Serialization` variant appropriate to the containing record. Store public methods do not expose `DomainInputError` directly and do not panic on invalid persisted input.

API parsing continues to convert `DomainInputError` and `TurnRequestError` into the same HTTP status class used before this refactor. No response body or event schema changes.

### 3.12 Test Contract

Add `crates/aise/tests/domain_core_dependency_tests.rs` using only public APIs. It contains exactly these behavioral cases:

| Test | Required assertion |
|---|---|
| `story_id_rejects_empty_and_blank` | Both invalid inputs return `DomainInputError::EmptyStoryId` and the exact display message |
| `turn_id_rejects_empty_and_blank` | Both invalid inputs return `DomainInputError::EmptyTurnId` and the exact display message |
| `constraint_id_rejects_empty_and_blank` | Both invalid inputs return `DomainInputError::EmptyConstraintId` and the exact display message |
| `domain_ids_preserve_string_serde_shape` | Valid round trips remain JSON strings; blank deserialization fails |
| `story_revision_preserves_integer_serde_shape` | `new`, `get`, `Display`, `Copy`, equality, and JSON integer round trip are unchanged |
| `legacy_and_v3_constraints_share_one_id_type` | One `domain::ids::ConstraintId` constructs both `StoryConstraint` field types |
| `trace_id_rejects_empty_and_blank_with_trace_error` | Both invalid inputs return `TraceIdError::EmptyTraceId` and the exact display message |
| `turn_request_errors_are_request_scoped` | Idempotency/player-input failures return the matching `TurnRequestError` variants |
| `turn_identity_constructor_is_infallible` | `TurnIdentity::new(...)` is assigned directly to `TurnIdentity` without Result handling |

Existing affected integration tests must migrate their imports and constructor handling rather than adding compatibility imports.

### 3.13 CI Boundary Check

Add this step to the `check` job in `.github/workflows/ci.yml` immediately after checkout and before formatting:

```yaml
- name: Domain dependency boundary
  shell: bash
  run: |
    violations="$(
      rg -n -U \
        '(?:crate|super(?:::\s*super)*)::\s*core\b|(?:crate|super)::\s*\{[^}]*\bcore(?:::|,|\})' \
        crates/aise/src/domain \
        --glob '*.rs' || true
    )"
    if [ -n "$violations" ]; then
      printf '%s\n' "$violations"
      exit 1
    fi
```

The check must print every offending file and line before failing. Add the identical step to the `msrv` job immediately after checkout and before its formatting step.

### 3.14 Affected File Manifest

The expected final change set includes:

```text
.github/workflows/ci.yml
crates/aise/src/core/turn_contract.rs
crates/aise/src/core/turn_event.rs
crates/aise/src/core/turn_trace.rs
crates/aise/src/domain/error.rs
crates/aise/src/domain/ids.rs
crates/aise/src/domain/mod.rs
crates/aise/src/domain/knowledge/fact.rs
crates/aise/src/domain/knowledge/memory.rs
crates/aise/src/domain/knowledge/query.rs
crates/aise/src/domain/knowledge/rumor.rs
crates/aise/src/domain/narrative_graph/director.rs
crates/aise/src/domain/narrative_graph/state.rs
crates/aise/src/domain/story_instance/binding.rs
crates/aise/src/domain/story_instance/snapshot.rs
crates/aise/src/domain/story_state.rs
crates/aise/src/engine.rs
crates/aise/src/persistence/sqlite_store.rs
crates/aise/src/persistence/store.rs
crates/aise/src/story/instance_factory.rs
crates/aise/src/validation/validation_pipeline.rs
crates/aise/src/context/tests/retrieval_pipeline_tests.rs
crates/aise/tests/character_think_pipeline_tests.rs
crates/aise/tests/core_turn_context_tests.rs
crates/aise/tests/domain_core_dependency_tests.rs
crates/aise/tests/llm_gateway_tests.rs
crates/aise/tests/narrative_graph_tests.rs
crates/aise/tests/persistence_tests.rs
crates/aise/tests/runtime_tests.rs
crates/aise/tests/story_instance_tests.rs
crates/aise/tests/validation_commit_tests.rs
crates/aise-server/tests/sse_tests.rs
```

This manifest is a minimum, not permission to leave compiler-reported old paths elsewhere. Additional affected test files must be migrated when static search or compilation finds them.

---

## 4. Behavior Rules

1. **R-1 — Zero Domain Backedges**: Every Rust file under `crates/aise/src/domain/` contains zero direct, grouped, aliased, re-exported, or qualified references to Core.
2. **R-2 — Single Persistent-Type Owner**: `StoryId`, `TurnId`, `StoryRevision`, and `ConstraintId` are defined exactly once, in `domain/ids.rs`.
3. **R-3 — No Library Session Type**: The `aise` crate contains no `SessionId`; only `aise-server::session::SessionId` remains.
4. **R-4 — Domain ID Validation**: `StoryId::try_new`, `TurnId::try_new`, and `ConstraintId::try_new` reject `""` and whitespace-only input with their matching `DomainInputError` variant.
5. **R-5 — Accepted ID Preservation**: A non-blank accepted ID is stored byte-for-byte as supplied, including leading or trailing whitespace.
6. **R-6 — Validated Deserialization**: Deserializing a Domain-owned ID invokes the same validation as `try_new`; direct private-field deserialization is forbidden.
7. **R-7 — Revision Compatibility**: `StoryRevision::new`, `get`, `Display`, equality, hashing, copy semantics, and integer Serde representation remain unchanged.
8. **R-8 — Shared Constraint Identity**: Both legacy and Story Pack v3 `StoryConstraint` structures use the same `domain::ids::ConstraintId` type.
9. **R-9 — Turn Identity Construction**: `TurnIdentity::new` returns `Self` and cannot fail; callers contain no obsolete result branch.
10. **R-10 — Error Ownership**: Domain ID errors use `DomainInputError`; request validation uses `TurnRequestError`; Trace ID parsing uses `TraceIdError`; Server Session parsing continues to use `SessionError`.
11. **R-11 — Correct Error Variants**: Empty `ConstraintId` returns `EmptyConstraintId`, and empty `TraceId` returns `EmptyTraceId`; neither returns `EmptyStoryId`.
12. **R-12 — Snapshot Stability**: Both snapshot types remain in Domain with unchanged data and accessor contracts; this refactor does not create `core::turn_data::StoryReadSnapshot`.
13. **R-13 — No Compatibility Layer**: Old definitions, old paths, old aliases, deprecated items, and `TurnInputError` are deleted in the same change.
14. **R-14 — Persistence Compatibility**: Existing valid IDs and revisions load and commit without migration, conversion, normalization, or rewrite.
15. **R-15 — Protocol Compatibility**: HTTP status classes, response fields, SSE payloads, Store result shapes, and Trace JSON shapes remain unchanged except that an empty Trace ID now reports `trace_id must not be empty`.
16. **R-16 — No External-Input Panic**: Domain constructors, Serde implementations, Store decoding, and API parsing do not use `unwrap`, `expect`, or `panic` for external or persisted input.
17. **R-17 — No Concurrency Change**: This refactor adds no lock, channel, task, future, `.await`, cache, queue, or shared mutable state.
18. **R-18 — CI Enforcement**: CI fails and prints locations when Domain introduces any direct or grouped Core reference.

### 4.1 Error Handling

- `StoryId::try_new("")` and `StoryId::try_new("   ")` return `DomainInputError::EmptyStoryId` with `story_id must not be empty`.
- `TurnId::try_new("")` and `TurnId::try_new("   ")` return `DomainInputError::EmptyTurnId` with `turn_id must not be empty`.
- `ConstraintId::try_new("")` and `ConstraintId::try_new("   ")` return `DomainInputError::EmptyConstraintId` with `constraint_id must not be empty`.
- `TraceId::try_new("")` and `TraceId::try_new("   ")` return `TraceIdError::EmptyTraceId` with `trace_id must not be empty`.
- Existing idempotency-key and player-input error strings and numeric fields remain exact under `TurnRequestError`.
- Persisted invalid IDs are converted to `StoreError::Serialization`; they are never converted to `StoreError::Unavailable` and never panic.
- API boundaries preserve current bad-request classification for invalid input.

### 4.2 Concurrency

- ID construction, error construction, revision access, and snapshot access remain synchronous.
- Existing Story serialization, Turn cancellation, Store transactions, Pipeline scheduling, and LLM concurrency behavior remain unchanged.
- No write guard or resource lifetime is added or extended.

### 4.3 Observability

- Do not add tracing spans, log events, or metrics for successful ID construction.
- Existing structured Turn, Store, API, and Trace event names and fields remain unchanged.
- Invalid external or persisted IDs remain diagnosable through their typed error and exact display message.
- No error may be silently discarded.
- The CI boundary check prints every matched file and line before returning a non-zero status.

---

## 5. Acceptance Criteria

### 5.1 Ownership and Static Dependency Checks

- [ ] The CI regex below returns zero matches:

  ```bash
  rg -n -U '(?:crate|super(?:::\s*super)*)::\s*core\b|(?:crate|super)::\s*\{[^}]*\bcore(?:::|,|\})' crates/aise/src/domain --glob '*.rs'
  ```

- [ ] `rg -n 'pub (struct|enum|type) (StoryId|TurnId|StoryRevision|SessionId)\b' crates/aise/src/core --glob '*.rs'` returns zero matches.
- [ ] `rg -n -U 'pub use [^;]*(StoryId|TurnId|StoryRevision|SessionId)' crates/aise/src/core --glob '*.rs'` returns zero matches.
- [ ] `rg -n '\bSessionId\b' crates/aise/src/core crates/aise/src/domain --glob '*.rs'` returns zero matches.
- [ ] `rg -n '\bTurnInputError\b' crates --glob '*.rs'` returns zero matches.
- [ ] `rg -n -U 'core::turn_contract::(?:\s*\{[^}]*\b(StoryId|TurnId|StoryRevision|SessionId)\b|\s*(StoryId|TurnId|StoryRevision|SessionId)\b)' crates --glob '*.rs'` returns zero matches.
- [ ] `rg -n 'pub struct (StoryId|TurnId|StoryRevision|ConstraintId)\b' crates/aise/src/domain/ids.rs` returns exactly four matches.
- [ ] `rg -n 'pub struct ConstraintId\b' crates/aise/src/domain --glob '*.rs'` returns exactly one match, in `domain/ids.rs`.
- [ ] `rg -n 'story_state::ConstraintId|story_instance::snapshot::ConstraintId' crates --glob '*.rs'` returns zero matches.
- [ ] `rg -n 'pub struct StoryReadSnapshot\b' crates/aise/src/domain --glob '*.rs'` returns exactly two matches, in `domain/story_state.rs` and `domain/story_instance/snapshot.rs`.
- [ ] `rg -n 'pub struct StoryReadSnapshot\b' crates/aise/src/core --glob '*.rs'` returns zero matches.
- [ ] `rg -n '_snapshot_anchor|_director_anchor' crates/aise/src/domain --glob '*.rs'` returns zero matches.
- [ ] `.github/workflows/ci.yml` contains the boundary check from §3.13 in every required CI path.

### 5.2 Contract Tests

- [ ] `cargo test -p aise --test domain_core_dependency_tests` passes all nine test cases from §3.12.
- [ ] Blank Domain ID deserialization fails for all three validated ID types.
- [ ] Valid Domain IDs serialize as plain JSON strings, not objects or arrays.
- [ ] `StoryRevision` serializes as a plain JSON integer.
- [ ] A single `ConstraintId` value type-checks in both legacy and v3 `StoryConstraint` values.
- [ ] `TurnIdentity::new` is called without Result handling in the contract test.

### 5.3 Call-Site and Compatibility Checks

- [ ] All `TurnIdentity::new` call sites compile without `.unwrap()`, `.expect()`, or an `Ok`/`Err` branch.
- [ ] All Domain files in §3.2 import IDs/revision exclusively from `domain::ids` or `super::ids`.
- [ ] `core::turn_contract`, `core::turn_event`, `story::instance_factory`, `persistence::store`, and `persistence::sqlite_store` use Domain-owned IDs/revision.
- [ ] `validation_pipeline.rs` constructs `domain::ids::ConstraintId` and retains its existing invariant failure mapping.
- [ ] `aise-server` production code continues to use only `aise_server::session::SessionId` for Sessions and `aise::domain::StoryId` for Stories.
- [ ] Existing SQLite databases require no migration; persistence integration tests pass without fixture rewrites.
- [ ] Existing API, SSE, Story Pack, Narrative Graph, Runtime, Trace, and Store tests pass without protocol-schema changes.
- [ ] `git diff -- crates/aise/src/domain/story_state.rs crates/aise/src/domain/story_instance/snapshot.rs` shows no snapshot field/accessor change beyond imports, shared `ConstraintId`, and deletion of the dead v3 anchor.

### 5.4 Toolchain

- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Consolidate the legacy and Story Pack v3 snapshots only after a separate design decides the authoritative Story Instance read model and Runtime migration boundary.
- Replace legacy Story state with the Story Pack v3 state model under a separate hard-refactor spec.
- Move deterministic state-transition methods and invariants into Domain under a separate Domain behavior spec.
- Remove Core dependencies on concrete configuration types under a separate configuration-boundary spec.
- Harden legacy `CharacterId`, `EventId`, `MemoryId`, `FactId`, and asset-key infallible conversions under a separate ID-validation spec.

---

## 7. References

- Source design dependency rules: [AISE Technical Architecture v3.1 §16](../design/2026-08-04-Architecture-gpt.md#16-分层与依赖方向)
- Source design snapshot contract: [AISE Technical Architecture v3.1 §10.1](../design/2026-08-04-Architecture-gpt.md#101-基础上下文)
- Story Pack v3 Domain layout: [Story Pack v3 Spec §3.1](./2026-08-07-story-pack-v3-spec-gpt.md#31-file-and-module-layout)
- Story Pack v3 snapshot contract: [Story Pack v3 Spec §3.11](./2026-08-07-story-pack-v3-spec-gpt.md#311-story-snapshot-contract)
- Superseded dependency-removal spec: [2026-08-06 version](./2026-08-06-domain-core-dependency-removal-spec-gpt.md)
- Superseded Turn input contract: [Turn Runtime Review Remediation Spec §3.1](./2026-08-06-turn-runtime-review-remediation-spec-gpt.md#31-validated-turn-input)
- Current Core-owned IDs/errors: `crates/aise/src/core/turn_contract.rs:13`, `crates/aise/src/core/turn_contract.rs:29`, `crates/aise/src/core/turn_contract.rs:52`, `crates/aise/src/core/turn_contract.rs:79`, `crates/aise/src/core/turn_contract.rs:156`
- Current Domain re-export backedge: `crates/aise/src/domain/ids.rs:5`
- Current legacy Story-state backedge: `crates/aise/src/domain/story_state.rs:1`, `crates/aise/src/domain/story_state.rs:31`
- Current Story Pack v3 backedges: `crates/aise/src/domain/story_instance/binding.rs:1`, `crates/aise/src/domain/story_instance/snapshot.rs:1`, `crates/aise/src/domain/knowledge/fact.rs:1`, `crates/aise/src/domain/knowledge/memory.rs:1`, `crates/aise/src/domain/knowledge/query.rs:1`, `crates/aise/src/domain/knowledge/rumor.rs:1`, `crates/aise/src/domain/narrative_graph/director.rs:1`, `crates/aise/src/domain/narrative_graph/state.rs:1`
- Current incorrect Trace error: `crates/aise/src/core/turn_trace.rs:25`
- Guardrails: [Architecture and Refactor](../agents/guardrails/architecture-refactor.md), [Layer Dependencies](../agents/guardrails/layer-dependencies.md), [Code Organization](../agents/guardrails/code-organization.md), [Errors and Observability](../agents/guardrails/observability.md)
