# Toolchain, Lints, and Dependencies

## R-LINT-01 - Format and lint clean

**Level: MUST**

- Code MUST pass `cargo fmt --check`.
- Code MUST pass `cargo clippy` with the project lint set; CI MUST treat
  warnings as errors (`-D warnings`).
- NEVER add `#[allow(...)]` without a brief justification comment on the same
  attribute.

```rust
// GOOD
#[allow(clippy::too_many_arguments)] // builder is generated; splitting hurts call sites
pub fn new(/* ... */) -> Self { /* ... */ }
```

---

## R-LINT-02 - `unsafe` policy

**Level: MUST**

- Crates MUST set `#![forbid(unsafe_code)]` by default.
- Any `unsafe` block MUST carry a waiver (`R-LINT-02`) and a `// SAFETY:`
  comment stating the invariant being upheld.
- NEVER use `unsafe` to silence a borrow-check or lifetime error that a safe
  refactor can fix.

---

## R-DEP-01 - Edition, MSRV, and dependencies

**Level: MUST for edition/MSRV; SHOULD for dependency selection**

- The workspace MUST pin a single Rust edition and a documented MSRV.
- New dependencies SHOULD be justified by maintenance status, license, and
  footprint; prefer crates already in the workspace.
- NEVER add a dependency for functionality that is trivially expressible in a
  few lines of safe std code.
