# AI Codegen - aise (Rust)

L1 entry: [AGENTS.md](../../AGENTS.md).

Scope: `crates/aise-core` and `crates/aise-service`. `crates/aise` and
`crates/aise-server` are frozen reference crates: read them for reference,
never modify them or depend on them.

---

## Document Structure

| Tier | Location | Load mode | Use |
|---|---|---|---|
| L1 | [AGENTS.md](../../AGENTS.md) | Auto-loaded | Core rules, doc routing, waiver |
| L2 | [guardrails/](./guardrails/) | On demand | Cross-cutting rules |
| L3 | `modules/` | On demand | Per-module rules, one file per aise-core module |

MUST open only the L2/L3 docs relevant to the current task.
NEVER load the whole L2/L3 set by default.

---

## L3 Module Map

One L3 file per top-level aise-core module (`crates/aise-core/src/`). Create
the file on demand when a module needs module-specific rules; until then the
path below is only a planned target. The architecture baseline is
[doc/design/2026-10-13-pipeline-framework-design-opus.md](../design/2026-10-13-pipeline-framework-design-opus.md).

| Module | L3 path | Coverage |
|---|---|---|
| `core` | `modules/core.md` | Domain model: story, role, narrative, knowledge, turn, IDs, errors |
| `engine` | `modules/engine.md` | `AiseEngine`, `AiseFactory`, `EngineConfig`, story creation, Turn entry |
| `llm` | `modules/llm.md` | `LlmGateway`, providers, concurrency limiter, LLM observations |
| `persistence` | `modules/persistence.md` | `Store`, `PackStore`, `StoryStore`, backends |
| `pipeline` | `modules/pipeline.md` | `Pipeline` trait, `PipelineRunner`, `Runtime`, stage modules |
| `prompt` | `modules/prompt.md` | Prompt library, `index.toml`, CSI/RC/FTI layers, rendering |
| `trace` | `modules/trace.md` | `Trace`, `Observation`, `ObservationSession`, content capture |

---

## Task -> Doc Table

| Task | Required doc |
|---|---|
| Reshape architecture / refactor a subsystem | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) |
| Add or change module/layer boundaries, add a pipeline stage, place config | [guardrails/layer-dependencies.md](./guardrails/layer-dependencies.md) |
| Add or change a pipeline stage, prompt, Turn flow, commit, or Validate/Repair loop | [guardrails/pipeline.md](./guardrails/pipeline.md) |
| Touch shared state, locks, or LLM call sites | [guardrails/concurrency.md](./guardrails/concurrency.md) |
| Write tests / add `mod.rs` / name types / add IDs / place config / format imports / enforce no comments / structure orchestration | [guardrails/code-organization.md](./guardrails/code-organization.md) |
| Add error handling, logging, observations, or events | [guardrails/observability.md](./guardrails/observability.md) |
| Set up the toolchain, lints, `unsafe`, or dependencies | [guardrails/toolchain.md](./guardrails/toolchain.md) |

---

## Rule ID Index

| Prefix | Category | File | Count |
|---|---|---|---|
| `R-ARCH-*` | Architecture baseline | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) | 5 |
| `R-REFACTOR-*` | Refactor baseline | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) | 2 |
| `R-LAYER-*` | Layer dependencies | [guardrails/layer-dependencies.md](./guardrails/layer-dependencies.md) | 5 |
| `R-CONC-*` | Concurrency and locks | [guardrails/concurrency.md](./guardrails/concurrency.md) | 4 |
| `R-CODE-*` | Code organization | [guardrails/code-organization.md](./guardrails/code-organization.md) | 8 |
| `R-OBS-*` | Errors and observability | [guardrails/observability.md](./guardrails/observability.md) | 6 |
| `R-LINT-*` | Toolchain and lints | [guardrails/toolchain.md](./guardrails/toolchain.md) | 2 |
| `R-DEP-*` | Dependencies and MSRV | [guardrails/toolchain.md](./guardrails/toolchain.md) | 1 |
| `R-AISE-*` | Turn pipeline constraints | [guardrails/pipeline.md](./guardrails/pipeline.md) | 8 |
| Total | | | 41 |
