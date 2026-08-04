# aise Codegen Guardrails (L1)

> Load this L1 file first. Open L2/L3 docs only when the current task matches
> the doc-routing section. Rule violations are P1 defects without a waiver.

---

## Core Rules

Use the rules below directly. Open detailed L2/L3 docs only by task.

Architecture and refactor baseline:
- `R-ARCH-01` MUST keep stable layers and one-way dependencies; NEVER add
  cross-layer shortcuts or backedges.
- `R-ARCH-02` MUST give every runtime object one clear owner and a bounded
  lifetime, with a shutdown path for background work.
- `R-ARCH-03` MUST budget hot paths for allocations, copies, concurrency,
  backpressure, and latency; NEVER add unbounded fan-out or hidden queues.
- `R-ARCH-04` MUST keep caches, queues, contexts, histories, and snapshots
  (including per-Turn context, character thoughts, and memory) bounded with a
  cleanup/eviction policy.
- `R-ARCH-05` MUST make failures diagnosable and key behavior observable and
  testable.
- `R-REFACTOR-01` MUST complete refactors in one change, with NO fallback
  branches, compatibility shims, adapter layers, or dual paths.
- `R-REFACTOR-02` MUST delete the old path in the same change: code, config,
  tests, docs, and dead flags.

Layers:
- `R-LAYER-01` Core/domain modules MUST NOT import transport, API, or adapter
  modules; cross-layer notifications MUST use injected traits, not concrete
  outer types.

Concurrency:
- `R-CONC-01` NEVER hold a write guard (`RwLock`/`Mutex`) across `.await` and
  NEVER return one; scope the lock to the smallest synchronous section.
- `R-CONC-03` NEVER emit events, send on channels, or do I/O while holding a
  write lock; mutate, drop the lock, then run side effects.
- `R-CONC-04` MUST route every LLM call (completion, streaming, embedding)
  through a shared, injected concurrency limiter.

Code organization:
- `R-CODE-01` MUST keep `mod.rs` and `lib.rs` as index only (module decls,
  re-exports, attributes); NEVER put functions, items, constants, or any other
  code there. Modules MUST use the directory layout (`foo/mod.rs` entry with
  sibling files in `foo/`); NEVER use a Rust 2018-style sibling `foo.rs`
  entry file.
- `R-CODE-02` MUST put unit tests in `tests/<source>_tests.rs`; NEVER inline
  `mod tests { ... }`.
- `R-CODE-05` MUST keep code comment-free: NEVER add `//` line comments, `///`
  doc comments, or `//!` module docs; the code is its own documentation. Sole
  exceptions: `// SAFETY:` on a waived `unsafe` block (`R-LINT-02`) and the
  `// WAIVER:` marker from the Waiver Process.
- `R-CODE-06` MUST keep runtime state and configuration separated; MUST roll
  configuration into typed `*Config` types.
- `R-CODE-07` MUST keep `use` imports compact: NEVER leave blank lines between
  consecutive `use` statements; the import block is one contiguous group at
  the top of the file. Sorting within the group is left to `rustfmt`
  (`R-LINT-01`).

Toolchain:
- `R-LINT-01` MUST pass `cargo fmt` and `clippy`; CI treats warnings as errors;
  any `#[allow]` MUST go through the waiver process instead of an inline
  justification comment.
- `R-LINT-02` Crates MUST set `#![forbid(unsafe_code)]` by default; any `unsafe`
  needs a waiver and a `// SAFETY:` comment (the only permitted `//` comment).
- `R-DEP-01` MUST pin a single edition and a documented MSRV; justify new
  dependencies.

Errors and observability:
- `R-OBS-01` MUST emit diagnosable errors on failure paths; NEVER fail silently.
- `R-OBS-02` MUST wrap LLM and tool calls in `tracing` spans with structured
  fields.
- `R-OBS-04` Logs MUST use structured fields for identifiers and error data;
  NEVER interpolate identifiers into message strings.
- `R-OBS-05` Core/domain MUST use typed (`thiserror`) errors and NEVER leak
  `anyhow::Error`; the app layer MAY use `anyhow`.

Project-specific hard constraints (see doc/design/2026-08-04-Architecture-gpt.md):
- `R-AISE-01` Turn execution MUST be orchestrated by `TurnRuntime`; pipelines
  MUST NOT call each other directly.
- `R-AISE-02` Every pipeline MUST implement `TurnExecutionPipeline` and
  interact only through `&mut TurnExecutionContext`; no hidden shared state.
- `R-AISE-03` `TurnExecutionContext` lives only for one Turn; it MUST NOT be
  persisted directly or shared across Turns.
- `R-AISE-04` Module boundaries MUST follow the architecture directory layout:
  `runtime` / `context` / `planning` / `character` / `story` / `validation` /
  `persistence` / `domain`. Cross-boundary imports follow `R-LAYER-01`.
- `R-AISE-05` `TurnCommitter` MUST guarantee atomicity, consistency, and
  recoverability of the Turn commit.
- `R-AISE-06` The Validation/Repair loop MUST have a bounded budget; on
  exhaustion, the Turn MUST fail with a diagnosable error, not loop forever.
- `R-AISE-07` Character thoughts are a character's viewpoint, not world facts;
  they MUST NOT be committed as world state by the generator.

---

## Which Docs to Read

Open only the L2/L3 docs relevant to the current change. Full index and
task -> doc table: [`doc/agents/README.md`](doc/agents/README.md).

- Reshape architecture or refactor a subsystem:
  [doc/agents/guardrails/architecture-refactor.md](doc/agents/guardrails/architecture-refactor.md).
- Change module or layer boundaries:
  [doc/agents/guardrails/layer-dependencies.md](doc/agents/guardrails/layer-dependencies.md).
- Touch shared state, locks, or LLM call sites:
  [doc/agents/guardrails/concurrency.md](doc/agents/guardrails/concurrency.md).
- Write tests, add `mod.rs`, name types, place config, format imports, or
  enforce no comments:
  [doc/agents/guardrails/code-organization.md](doc/agents/guardrails/code-organization.md).
- Add error handling, logging, tracing, or events:
  [doc/agents/guardrails/observability.md](doc/agents/guardrails/observability.md).
- Set up the toolchain, lints, `unsafe`, or dependencies:
  [doc/agents/guardrails/toolchain.md](doc/agents/guardrails/toolchain.md).

---

## Waiver Process

1. Commit message MUST include `Waive: R-XXX-NN reason=<brief reason>`.
2. Code MUST mark the exact boundary with `// WAIVER: R-XXX-NN`.

Undocumented violations are P1 defects.
