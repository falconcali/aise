# Errors and Observability

## R-OBS-01 - Failure paths are diagnosable

**Level: MUST**

- Parse, LLM, tool, and I/O failures MUST emit a structured error event or
  return a diagnosable error before unwinding.
- Code MUST NOT silently drop errors.

```rust
// BAD
match analyze_intent(&ctx).await {
    Ok(intent) => intent,
    Err(_) => return,
}

// GOOD
match analyze_intent(&ctx).await {
    Ok(intent) => intent,
    Err(e) => {
        tracing::error!(stage = "intent", error = %e, "intent analysis failed");
        return Err(e.into());
    }
}
```

---

## R-OBS-02 - LLM and tool spans

**Level: MUST**

- LLM calls MUST run inside `tracing` spans with structured fields.
- Tool execution MUST run inside a span.
- New LLM providers and tools MUST NOT bypass spans.

```rust
// GOOD
let span = tracing::info_span!(
    "llm.complete",
    model = %self.model_name(),
    turn_id = %ctx.turn_id
);
async move { /* ... */ }.instrument(span).await
```

---

## R-OBS-03 - Non-fatal errors warn and continue

**Level: SHOULD**

- Non-fatal errors SHOULD use `warn!` and continue.
- Non-fatal errors SHOULD NOT abort the full turn or session.

```rust
// GOOD
if let Err(e) = memory_pipeline.spawn_extraction(...).await {
    tracing::warn!(turn_id = %ctx.turn_id, error = %e, "memory extraction failed");
}
```

---

## R-OBS-04 - Structured log fields

**Level: MUST**

- Logs MUST use structured fields for identifiers and error data.
- Logs MUST NOT interpolate identifiers into message strings.

```rust
// BAD
tracing::info!("turn {} handled event for session {}", turn_id, session_id);

// GOOD
tracing::info!(
    session_id = %session_id,
    turn_id = %turn_id,
    "turn handled event"
);
```

---

## R-OBS-05 - Panic only for invariants

**Level: SHOULD**

- `panic!`, `unwrap`, and `expect` SHOULD be used only for broken invariants.
- Business errors MUST use `Result`.
- turn/domain layers SHOULD define typed errors with `thiserror` so callers can
  discriminate; NEVER leak `anyhow::Error` out of a domain API.
- Binary/app/composition layers MAY use `anyhow` to aggregate and report errors.

```rust
// BAD
let character = state.characters.get(&id).unwrap();

// GOOD
let character = state
    .characters
    .get(&id)
    .ok_or_else(|| anyhow::anyhow!("character {id} not found"))?;
```
