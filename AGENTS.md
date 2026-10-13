# aise Codegen Guardrails (L1)

> Load this L1 file first. Open L2/L3 docs only when the current task matches
> the doc-routing section. Rule violations are P1 defects without a waiver.

---

## Scope

- Maintained crates: `crates/aise-core` (library) and `crates/aise-service`
  (binary). Every rule below applies to them.
- Frozen reference crates: `crates/aise` and `crates/aise-server`. MAY be read
  for reference; NEVER modify them, and NEVER make a maintained crate depend
  on them.
- Architecture baseline:
  [doc/design/2026-10-13-pipeline-framework-design-opus.md](doc/design/2026-10-13-pipeline-framework-design-opus.md).

---

## Core Rules

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

Layers (`core`, `trace` are leaves -> `llm` / `prompt` / `persistence` ->
`pipeline` -> `engine` -> `aise-service`):
- `R-LAYER-01` Inner modules MUST NOT import outer modules; notifications
  toward callers MUST use injected traits (e.g. `TurnEventSink`).
- `R-LAYER-03` Pipeline stages MUST NOT import each other; types shared by
  stages live in `core` or `pipeline::common`; only `pipeline::runtime` imports
  stage modules.

Turn pipeline:
- `R-AISE-01` `Runtime::run_turn` is the single Turn orchestrator with a fixed
  stage topology; pipelines MUST NOT call each other.
- `R-AISE-05` Only the commit stage MAY write Turn results, atomically and
  without calling an LLM.
- `R-AISE-06` The Validate/Repair loop MUST have a `Runtime`-owned budget and
  end with a diagnosable outcome on exhaustion.

Concurrency:
- `R-CONC-01` NEVER hold a write guard (`RwLock`/`Mutex`) across `.await` and
  NEVER return one; scope the lock to the smallest synchronous section.
- `R-CONC-03` NEVER emit events, send on channels, or do I/O while holding a
  write lock; mutate, drop the lock, then run side effects.
- `R-CONC-04` MUST route every LLM call through the injected `LlmGateway`,
  which owns the shared concurrency limiter; NEVER call an `LlmProvider`
  directly.

Code organization:
- `R-CODE-01` MUST keep `mod.rs` and `lib.rs` as index only (module decls,
  re-exports, attributes) and use the `foo/mod.rs` directory layout, never a
  sibling `foo.rs` entry file.
- `R-CODE-02` MUST put unit tests in `tests/<source>_tests.rs`; NEVER inline
  `mod tests { ... }`.
- `R-CODE-04` MUST declare every ID type with `define_id!` in `core/ids.rs`;
  NEVER use a bare `String` for an ID.
- `R-CODE-05` MUST keep code comment-free (no `//`, `///`, `//!`); sole
  exceptions are `// SAFETY:` (`R-LINT-02`) and `// WAIVER:`.
- `R-CODE-06` MUST keep runtime state and configuration separated; each module
  owns its typed `*Config`, all rolled up into `EngineConfig`.
- `R-CODE-07` MUST keep `use` imports as one contiguous block at the top of
  the file, with no blank lines between them.
- `R-CODE-08` MUST keep orchestration functions at one abstraction level;
  tracing, metrics, logging, timing, serialization, and observability-only
  outcome mapping MUST live behind named helpers or owned abstractions.

Toolchain:
- `R-LINT-01` MUST pass `cargo fmt` and `clippy` with warnings as errors; any
  `#[allow]` MUST go through the waiver process.
- `R-LINT-02` Crates MUST set `#![forbid(unsafe_code)]`; any `unsafe` needs a
  waiver and a `// SAFETY:` comment.
- `R-DEP-01` MUST pin a single edition and a documented MSRV; justify new
  dependencies.

Errors and observability:
- `R-OBS-01` MUST emit diagnosable errors on failure paths; NEVER fail silently.
- `R-OBS-02` LLM calls and stage work MUST be observed through
  `trace::Observation` under an explicitly passed parent; pipelines MUST NOT
  create ad-hoc `tracing` spans.
- `R-OBS-04` Logs MUST use structured fields for identifiers and error data;
  NEVER interpolate identifiers into message strings.
- `R-OBS-05` aise-core MUST use typed (`thiserror`) errors and NEVER leak
  `anyhow::Error`; `aise-service` MAY use `anyhow`.

---

## Which Docs to Read

Open only the L2/L3 docs matching the current task; the task -> doc table and
rule index are in [`doc/agents/README.md`](doc/agents/README.md). Adding or
changing a pipeline stage, prompt, or the Turn flow MUST read
[doc/agents/guardrails/pipeline.md](doc/agents/guardrails/pipeline.md).

---

## Waiver Process

1. Commit message MUST include `Waive: R-XXX-NN reason=<brief reason>`.
2. Code MUST mark the exact boundary with `// WAIVER: R-XXX-NN`.

Undocumented violations are P1 defects.
