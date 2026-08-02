# {Topic in Title Case} — Spec

> **Model**: {model short name, e.g. Opus / GPT-5.4 / Sonnet}
> **Date**: {YYYY-MM-DD}
> **Status**: Proposed
> **Source Design**: [{design doc title}]({relative path, e.g. ../design/YYYY-MM-DD-xxx-design-{model}.md})
> **Phase**: {optional, e.g. Phase 0 / Phase 1 / N-A}

---

## 1. Goal

<!--
One or two sentences. Concrete and verb-driven.
BAD:  "Improve the tool system."
GOOD: "Unify local and worker tool execution behind a single ToolDefinition and ToolResultEnvelope contract."
-->

{single-sentence goal}

---

## 2. Scope & Non-Goals

### 2.1 In Scope

<!-- Concrete deliverables. Every bullet must be verifiable by code review. -->

- {deliverable 1}
- {deliverable 2}
- {deliverable 3}

### 2.2 Non-Goals

<!--
REQUIRED. List what this spec deliberately does NOT do.
This prevents scope creep during AI code generation.
-->

- {non-goal 1, e.g. "Does not introduce MCP integration"}
- {non-goal 2}

### 2.3 Implementation Constraints (for code generation)

<!--
AISE defaults to hard refactors (R-REFACTOR-01/02). State it explicitly.
-->

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, or dual-write logic unless called out below.
- Old types / functions / modules superseded by this spec MUST be deleted, not deprecated.
- No mid-state "both systems coexist" phase.

{list any exceptions here, e.g. "Exception: `LegacyFoo::parse_v1` is retained until Phase 3."}

---

## 3. Contracts

<!--
The meat of the spec. Give explicit type signatures, function signatures,
event shapes, JSON schemas — whatever the AI needs to produce correct code.
NO prose-only descriptions; always show the concrete shape.
-->

### 3.1 Types

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
    pub meta: ToolMeta,
}

pub struct ToolResultEnvelope {
    pub ok: bool,
    pub payload: Value,
    pub display: Option<ToolResultDisplay>,
    pub meta: ToolResultMeta,
}
```

### 3.2 Functions / APIs

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    async fn invoke(&self, args: Value, ctx: &ToolCtx) -> Result<ToolResultEnvelope, ToolError>;
}
```

### 3.3 Events / Protocol

```json
{
  "kind": "tool.completed",
  "tool_call_id": "string",
  "envelope": { "...": "..." }
}
```

### 3.4 File / Directory Layout

```
src/core/tool_runtime/
├── definition.rs
├── envelope.rs
├── registry.rs
└── invoke.rs
```

---

## 4. Behavior Rules

<!--
Precise rules the AI must follow. Use numbered items so they can be cited.
Every rule must be testable.
-->

1. **R-1**: {rule with exact condition and expected behavior}
2. **R-2**: {rule}
3. **R-3**: {rule}

### 4.1 Error Handling

- On {condition}, return `ToolError::{Variant}` with `message = "{exact string}"`.
- Never `.unwrap()` on external input; always `?` into `ToolError::BadRequest`.

### 4.2 Concurrency

- {rule, e.g. "Registry is `Arc<dyn Tool>` keyed by name; cloneable; no write lock in the hot path." (R-CONC-01)}

### 4.3 Observability

- Emit `tracing::info_span!("tool.invoke", tool = %name, call_id)`. (R-OBS-02)
- Counter `npc_tool_invocations_total{tool, status}` incremented per call.

---

## 5. Acceptance Criteria

<!--
A checklist an AI or human reviewer can mechanically verify.
Each item ends with a concrete check (test name / rg query / curl call).
-->

- [ ] `ToolDefinition` at `{path}` matches §3.1 exactly
- [ ] `Tool` trait signature matches §3.2
- [ ] `cargo test tool_runtime::` passes
- [ ] `rg 'OldToolAbstraction' src/` returns zero matches (old code fully removed)
- [ ] Event `tool.completed` emitted with envelope shape §3.3 — verified by `cargo test tool_events::completed_shape`
- [ ] Metric `npc_tool_invocations_total` visible via `/metrics`

---

## 6. Out of Scope / Future Work

<!--
Optional. Items deferred to a later spec or phase. Each links to a follow-up.
-->

- {deferred item} → handled in `doc/exec/{path-to-next-spec}.md`
- {deferred item} → tracked in `doc/TODO.md`

---

## 7. References

- Source design: {link}
- Related prior art: {link to existing code / external source}
- Guardrails: `doc/agents/guardrails/`
