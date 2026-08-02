# Concurrency and Locks

## R-CONC-01 - No write lock across await

**Level: MUST**

- NEVER hold a write guard (`RwLockWriteGuard`, `MutexGuard`) across `.await`.
- NEVER return a write guard from a function.
- MUST scope the lock to the smallest synchronous critical section, then drop it
  before any async work.

```rust
// BAD
let mut w = state.write().await;
let pipeline = w.pipelines.get(&id).unwrap().clone();
pipeline.execute(&mut ctx).await; // lock held across .await
w.turn_counter += 1;

// GOOD
{
    let mut w = state.write().await;
    w.turn_counter += 1;
}
let pipeline = { state.read().await.pipelines.get(&id).cloned() };
if let Some(pipeline) = pipeline {
    pipeline.execute(&mut ctx).await?;
}
```

---

## R-CONC-02 - Keep read locks short

**Level: SHOULD**

- SHOULD keep read locks short.
- SHOULD clone the needed data and release the lock before I/O, network calls,
  or `.await`.

```rust
// GOOD
let snapshot = {
    let s = state.read().await;
    s.characters.get(&id).cloned()
};
if let Some(character) = snapshot {
    character.think(&ctx).await?;
}
```

---

## R-CONC-03 - No side effects under a write lock

**Level: MUST**

- NEVER emit events, send on channels, or perform I/O while holding a write
  lock.
- MUST apply the state mutation, drop the lock, then perform side effects.

```rust
// BAD
let mut s = state.write().await;
s.apply_mutation();
event_tx.send(Event::Updated { .. });

// GOOD
{
    let mut s = state.write().await;
    s.apply_mutation();
}
event_tx.send(Event::Updated { .. });
```

---

## R-CONC-04 - Route LLM calls through a shared limiter

**Level: MUST**

- Every LLM call (completion, streaming, embedding) MUST acquire a shared
  concurrency limiter (e.g. a `tokio::sync::Semaphore`) before dispatch.
- NEVER add an LLM call site that bypasses the limiter.
- The limiter MUST be owned at the application root and injected, so backpressure
  and provider rate limits stay centrally enforced.
