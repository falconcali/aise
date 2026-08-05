# {Issue title}

> **Date**: {YYYY-MM-DD}
> **Reporter**: {full model name or user}
> **Severity**: P1 / P2 / P3
> **Status**: Open / In-Progress / Resolved
> **Module**: {module / file path}

---

## 1. Symptom

<!--
What the user or test saw.
Cover: trigger scenario + observed behavior + expected behavior.
Avoid "there's a bug".
-->

**Trigger**: {when it happens}

**Actual**: {what is observed}

**Expected**: {what it should be}

---

## 2. Reproduction

<!--
If reproducible, list steps. If flaky, state the rate + known triggers.
-->

1. {step 1}
2. {step 2}
3. {step 3}

**Repro rate**: {stable / intermittent N%}

**Environment**:

- aise version / commit: {sha or tag}
- OS:
- Model:
- Other relevant config:

---

## 3. Impact

- **Feature**: {which features are affected}
- **Users**: {how many, what kind}
- **Data**: {data corruption? recoverable?}
- **Security**: {permissions / privacy / injection involved?}

---

## 4. Root Cause (optional)

<!--
If located, write it; otherwise leave "not located".
Format: code location + faulty logic + why it gets there.
-->

**Location**: `{path:line}`

**Root cause**: {what is wrong}

```rust
// {path:line}
{key snippet}
```

---

## 5. Proposed Fix

### Short-term workaround

{client- or config-side workaround; "none" if none}

### Long-term fix

{the real fix direction; if large, point to a separate refactor/design doc}

---

## 6. Related

- Related issue / PR: {link}
- Related docs: {link}
- Screenshots / logs: {path or paste}
