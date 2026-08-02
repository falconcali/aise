# Code Organization

## R-CODE-01 - `mod.rs` and `lib.rs` are index only

**Level: MUST**

- `mod.rs` and `lib.rs` files may contain only module declarations (`mod`,
  `pub mod`), visibility re-exports (`pub use`, `pub(crate) use`,
  `pub(super) use`), attributes on those items (`#[cfg]`, `#[allow]`,
  `#![forbid(unsafe_code)]`), and module docs (`//!`).
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

//! `gateway` crate root.

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

## R-CODE-05 - Public API comments

**Level: MUST**

- MUST add `///` doc comments only when the public contract is not obvious from
  the name, type, and module context.
- MUST document non-obvious ownership, lifetime, locking, side effects,
  persistence, I/O, invariants, defaults, units, ranges, and caller
  obligations.
- NEVER add doc comments that restate the signature or narrate implementation.
- SHOULD omit doc comments for obvious constructors, getters, setters, one-line
  forwards, and self-explanatory data carriers.

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

## R-CODE-07 - Code comments

**Level: MUST**

- Comments MUST be concise English.
- Comments MUST explain non-obvious why, not obvious what.
- NEVER add empty commentary such as "Import the module", "Define the
  function", or "Return the result".

```rust
// BAD
// increment counter
self.turn_counter += 1;

// GOOD
// Tick is visible to the sweeper; avoids unloading a character mid-turn.
self.turn_counter += 1;
```

---

## R-CODE-08 - Module-level `//!` docs

**Level: SHOULD**

- `lib.rs` and every `mod.rs` SHOULD carry a `//!` doc stating the module's
  role and boundary (what it owns, what it does NOT own).
- Regular source files MAY omit `//!`; add one only when the file represents
  a stable concept whose responsibility is not obvious from its name.
- Keep `//!` to a few sentences. NEVER duplicate the design doc, `README`, or
  `AGENTS.md`; link to them by path instead.
- NEVER use `//!` to narrate implementation or list every item in the file.

```rust
// GOOD - lib.rs
//! `gateway` crate root.
//!
//! Hosts the `gateway-server` binary's library surface. Per `R-CODE-01`, this
//! file is an index only: module declarations, re-exports, attributes, and
//! `//!` docs. No items, no functions, no business logic.
```

```rust
// BAD - restates what readers can see
//! This module contains `GatewayConfig`. It has a `from_env` function that
//! reads environment variables and returns a `GatewayConfig`.
```
