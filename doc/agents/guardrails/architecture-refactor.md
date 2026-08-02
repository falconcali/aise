# Architecture and Refactor Baseline

## R-ARCH-01 - Stable layers

**Level: MUST**

- MUST keep responsibilities in the owning layer.
- MUST keep dependencies one-way.
- NEVER add cross-layer shortcuts or backedges.
- SHOULD minimize the number of files touched by new behavior.

This is the principle; for concrete import and injection rules see
[layer-dependencies.md](./layer-dependencies.md) (`R-LAYER-*`).

## R-ARCH-02 - Ownership and lifetime

**Level: MUST**

- MUST give every runtime object one clear owner.
- MUST give every runtime object a bounded lifetime.
- NEVER create leaked tasks, orphaned channels, or duplicate ownership.
- MUST provide a shutdown path for background work.

## R-ARCH-03 - Hot path budgets

**Level: MUST**

- MUST budget hot paths for allocations, copies, concurrency, backpressure, and
  latency.
- NEVER add unbounded fan-out.
- NEVER add hidden queues.
- NEVER add per-request work that scales with total system size.

## R-ARCH-04 - Bounded memory

**Level: MUST**

- MUST define size limits for caches, histories, queues, snapshots, and
  contexts (including per-Turn context, character thoughts, and memory).
- MUST define a cleanup or eviction policy for bounded memory.
- NEVER introduce unbounded memory growth.

## R-ARCH-05 - Diagnosable failures

**Level: MUST**

- MUST emit actionable errors for critical flows.
- MUST use structured logs for production-relevant behavior.
- MUST make key behavior observable and testable.

This is the principle; for concrete error, logging, and tracing rules see
[observability.md](./observability.md) (`R-OBS-*`).

---

## R-REFACTOR-01 - Complete refactors

**Level: MUST**

- MUST replace the old structure in the same change.
- NEVER leave fallback branches, compatibility shims, adapter layers, or dual
  paths without a waiver.

## R-REFACTOR-02 - Delete old paths

**Level: MUST**

- MUST remove superseded code, config, docs, tests, and dead flags in the same
  change.
- NEVER keep temporary parallel paths without a waiver.
