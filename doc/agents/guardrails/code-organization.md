# Code Organization

## R-CODE-01 - `mod.rs` and `lib.rs` are index only

**Level: MUST**

- `mod.rs` and `lib.rs` files may contain only module declarations (`mod`,
  `pub mod`), visibility re-exports (`pub use`, `pub(crate) use`,
  `pub(super) use`), and attributes on those items (`#[cfg]`, `#[allow]`,
  `#![forbid(unsafe_code)]`).
- NEVER put structs, enums, traits, impls, functions, constants, statics, type
  aliases, or business logic in `mod.rs` or `lib.rs`.
- MUST put code in dedicated files and re-export from `mod.rs` / `lib.rs`.
- Modules MUST use the classic directory layout: index in `foo/mod.rs` with
  sibling files under `foo/`. NEVER use a Rust 2018-style `foo.rs` entry file
  with a sibling `foo/` directory; migrate any existing `foo.rs` entries to
  `foo/mod.rs`.
- Binary bootstrap (tracing init, signal wiring, config loading glue) belongs
  in `main.rs`, NEVER in `lib.rs`.

```rust
// GOOD - engine/mod.rs
mod config;
mod engine;
mod error;
mod factory;

pub use config::EngineConfig;
pub use engine::{AiseEngine, Engine};
pub use error::EngineError;
pub use factory::{AiseFactory, RoleActorSpec, StoryCreationSpec};
```

---

## R-CODE-02 - Unit tests use dedicated files

**Level: MUST**

- MUST put unit tests in `tests/<source>_tests.rs`.
- NEVER use inline `#[cfg(test)] mod tests { ... }` blocks.
- Source files MUST declare only the external test module, and it MUST be the
  last item in the file:

```rust
#[cfg(test)]
#[path = "tests/plan_prompt_tests.rs"]
mod tests;
```

```text
src/pipeline/plan/
  plan.rs
  plan_prompt.rs
  tests/
    plan_prompt_tests.rs
```

- Test files MUST start with `use super::*;`.
- Test files MAY add `use crate::...` as needed.
- MUST use one `#[test]` or `#[tokio::test]` per case.
- NEVER nest `mod` inside test files.
- MUST use descriptive test names.
- SHOULD skip unit tests for `mod.rs`, `main.rs`, and pure derive-only structs.
- Integration tests MUST live under the crate `tests/` directory and test only
  public APIs.

---

## R-CODE-03 - Naming

**Level: SHOULD**

- Files, folders, functions, and fields SHOULD use `snake_case`.
- Types and traits SHOULD use `PascalCase`.
- Constants and statics SHOULD use `SCREAMING_SNAKE_CASE`.
- Lifetimes SHOULD use single letters such as `'a` and `'b`.
- API view and snapshot structs SHOULD use an `XxxInfo` suffix (e.g.
  `StoryInstanceInfo`).
- Configuration structs SHOULD use an `XxxConfig` suffix.
- Write/command argument bundles SHOULD use an `XxxSpec` suffix (e.g.
  `StoryInstanceSpec`, `LlmCompletionSpec`).
- Pipeline stage types SHOULD be named `{Stage}Pipeline`, `{Stage}Input`, and
  `{Stage}Output` (e.g. `PlanPipeline`, `PlanInput`, `PlanOutput`).

---

## R-CODE-04 - ID types use `define_id!` newtypes

**Level: MUST**

- Every ID type MUST be declared with the `define_id!` macro in
  `crates/aise-core/src/core/ids.rs`; NEVER use a bare `String` for an ID.
- `define_id!` produces a `String` newtype whose `try_new` rejects empty or
  whitespace-only values and returns `CoreError::InvalidId`.
- Different ID domains MUST NOT share the same type.
- NEVER hand-write a separate ID struct outside `define_id!`.

```rust
define_id!(StoryId, "story_id");
define_id!(RoleId, "role_id");
```

---

## R-CODE-05 - No comments in code

**Level: MUST**

- NEVER add `//` line comments, `///` doc comments, or `//!` module docs to any
  code. The code itself is the only documentation.
- Sole exceptions: `// SAFETY:` on a waived `unsafe` block (see `R-LINT-02`)
  and the `// WAIVER:` marker from the Waiver Process.
- NEVER restate the signature or narrate the implementation.

---

## R-CODE-06 - Config and state separation

**Level: MUST for placement; SHOULD for naming**

- Each module owns its typed `*Config` in its own `config.rs`; all module
  configs MUST roll up into `EngineConfig`, the single root config (see
  `R-LAYER-05`).
- Configuration MUST NOT hang off runtime state containers directly; runtime
  objects receive the config they need at construction time.
- Mutable runtime state MUST live in dedicated state containers, never mixed
  into config types.
- Configuration structs SHOULD use the `XxxConfig` suffix.
- Read snapshots SHOULD use the `XxxInfo` suffix.

---

## R-CODE-07 - Compact `use` imports

**Level: MUST**

- The `use` imports at the top of a file MUST form one contiguous block:
  NEVER leave blank lines between consecutive `use` statements.
- The import block MUST sit at the very top of the file, followed by a single
  blank line before the first item.
- Sorting inside the block MUST be delegated to `rustfmt` (`R-LINT-01`); do
  NOT hand-sort or split imports into groups.

```rust
// GOOD - one contiguous import block
use crate::core::{BoundedText, PlayerContribution};
use serde::Serialize;

#[derive(Serialize)]
pub struct PlanInput {
    // ...
}
```

```rust
// BAD - blank line inside the import block
use crate::core::{BoundedText, PlayerContribution};

use serde::Serialize;
```

---

## R-CODE-08 - Orchestration functions stay at one abstraction level

**Level: MUST**

- Orchestration functions MUST show the primary flow in execution order and at
  one abstraction level.
- Supporting mechanics MUST live behind named helpers or owned abstractions;
  concise lifecycle calls MAY remain.
- Business decisions, state changes, and error semantics MUST remain explicit.

```rust
// BAD - observation mechanics obscure the business operation
let plan_observation = observation.begin(ObservationSpec {
    name: "plan",
    kind: ObservationKind::Span,
    input: observation.capture_content(&input.player_contribution.processed),
    metadata: Vec::new(),
});
let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
plan_observation.finish(match &result {
    Ok(plan) => ObservationOutcome { status: ObservationStatus::Ok, /* ... */ },
    Err(error) => ObservationOutcome { status: ObservationStatus::Error, /* ... */ },
});
```

```rust
// GOOD - lifecycle stays visible; mechanics live in plan_trace.rs
let plan_observation = plan_trace::begin_plan(observation, &input.player_contribution.processed);
let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
plan_trace::finish_plan(plan_observation, &result);
```
