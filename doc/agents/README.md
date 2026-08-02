# AI Codegen - aise (Rust)

L1 entry: [AGENTS.md](../../AGENTS.md).

---

## Document Structure

| Tier | Location | Load mode | Use |
|---|---|---|---|
| L1 | [AGENTS.md](../../AGENTS.md) | Auto-loaded | Core rules, doc routing, waiver |
| L2 | [guardrails/](./guardrails/) | On demand | Cross-cutting rules |
| L3 | modules/ (TBD) | On demand | Subsystem rules, added as the architecture lands |

MUST open only the L2/L3 docs relevant to the current task.
NEVER load the whole L2/L3 set by default.

---

## Task -> Doc Table

| Task | Required doc |
|---|---|
| Reshape architecture / refactor a subsystem | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) |
| Add or change module/layer boundaries | [guardrails/layer-dependencies.md](./guardrails/layer-dependencies.md) |
| Touch shared state, locks, or LLM call sites | [guardrails/concurrency.md](./guardrails/concurrency.md) |
| Write tests / add `mod.rs` / name types / place config / adjust comments | [guardrails/code-organization.md](./guardrails/code-organization.md) |
| Add error handling, logging, tracing, or events | [guardrails/observability.md](./guardrails/observability.md) |
| Set up the toolchain, lints, `unsafe`, or dependencies | [guardrails/toolchain.md](./guardrails/toolchain.md) |

---

## Rule ID Index

| Prefix | Category | File | Count |
|---|---|---|---|
| `R-ARCH-*` | Architecture baseline | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) | 5 |
| `R-REFACTOR-*` | Refactor baseline | [guardrails/architecture-refactor.md](./guardrails/architecture-refactor.md) | 2 |
| `R-LAYER-*` | Layer dependencies | [guardrails/layer-dependencies.md](./guardrails/layer-dependencies.md) | 2 |
| `R-CONC-*` | Concurrency and locks | [guardrails/concurrency.md](./guardrails/concurrency.md) | 4 |
| `R-CODE-*` | Code organization | [guardrails/code-organization.md](./guardrails/code-organization.md) | 7 |
| `R-OBS-*` | Errors and observability | [guardrails/observability.md](./guardrails/observability.md) | 5 |
| `R-LINT-*` | Toolchain and lints | [guardrails/toolchain.md](./guardrails/toolchain.md) | 2 |
| `R-DEP-*` | Dependencies and MSRV | [guardrails/toolchain.md](./guardrails/toolchain.md) | 1 |
| Total | | | 28 |
