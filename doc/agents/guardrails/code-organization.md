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
- For new modules, MUST prefer the Rust 2018 style (`foo.rs` with a sibling
  `foo/` directory) over `foo/mod.rs`; reserve `mod.rs` for legacy modules and
  migrate it when substantially touched.
- Binary bootstrap (tracing init, signal wiring, config loading glue) belongs
  in `main.rs`, NEVER in `lib.rs`.

```rust
// GOOD - lib.rs
#![forbid(unsafe_code)]

pub mod app;
pub mod config;
pub mod error;

pub use app::GatewayApp;
pub use config::GatewayConfig;
pub use error::GatewayError;
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
#[path = "tests/dialogue_tests.rs"]
mod tests;
```

```text
src/character/
  character_think.rs
  memory.rs
  tests/
    character_think_tests.rs
    memory_tests.rs
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
  `CharacterInfo`, `TurnInfo`).
- Configuration structs SHOULD use an `XxxConfig` suffix.
- Write/command argument bundles SHOULD use an `XxxSpec` suffix.

---

## R-CODE-04 - ID types use newtypes

**Level: SHOULD**

- IDs SHOULD use newtypes over `Arc<str>` rather than bare `String`.
- Different ID domains MUST NOT share the same type.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharacterId(Arc<str>);
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

- Configuration MUST NOT hang off runtime state containers directly; it MUST
  roll up into a typed root config (e.g. `AppConfig`).
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
use crate::domain::ids::CharacterId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterThought {
    // ...
}
```

```rust
// BAD - blank line inside the import block
use crate::domain::ids::CharacterId;

use serde::{Deserialize, Serialize};
```
