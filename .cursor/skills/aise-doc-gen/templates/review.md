# {Review target} — Review

> **Target**: {module path / project phase / PR range}
> **Date**: {YYYY-MM-DD}
> **Reviewer**: {full model name, e.g. Opus (Claude) / GPT-5.4}
> **Standard**: {standard doc relied on, e.g. `doc/review/architecture-review/xxx.md`; if none, "no unified standard, reviewed by judgment"}
> **Coverage**: {sampled / full; if sampled, state the sampling strategy}

---

## 1. Overall Verdict

<!--
3–5 lines. Conclusion first, then reasons.
Cover: current phase, key strengths, key risks.
-->

**Verdict**: {one-line conclusion}

**Current phase**: {exploration / transition / production / ...}

**Top systemic problems to fix first**:

1. {problem 1}
2. {problem 2}
3. {problem 3}

---

## 2. Red Lines

<!--
A red line = must fix; shipping without fixing risks a production incident.
Keep the count small, usually ≤ 5; more means the whole thing is unhealthy.
-->

| Red line | Dimension / location | Severity | Why |
|---|---|---|---|
| {problem} | {path:line or "concurrency model"} | high | {why it is a red line} |
| {problem} | | med-high | |

---

## 3. Per-Dimension Scores (optional)

<!--
If a unified dimension set exists, score it. Otherwise switch to per-module analysis.
-->

| Dimension | Score (1–5) | Reason |
|---|---|---|
| Responsibility boundaries & layering | {N} | {one line} |
| Change cost & coupling | {N} | |
| Error handling & safety | {N} | |
| Observability | {N} | |
| Tests & verification | {N} | |

---

## 4. Detailed Analysis

<!--
By module or by red line. Each item: state → evidence → impact → recommendation.
-->

### 4.1 {problem or module name}

**State**: {objective description}

**Evidence**:

```rust
// {path:line}
{key snippet}
```

**Impact**: {what it leads to}

**Recommendation**: {how to fix; if large, point to a separate refactor doc}

### 4.2 {…}

---

## 5. Strengths

<!--
Record what is worth keeping / spreading, so it is not deleted by mistake later.
-->

- {strength 1: what it is and why it is good}
- {strength 2}

---

## 6. Recommendations

<!--
Prioritized, batched into a remediation plan.
-->

### P1 (must do, blocking)

- {item} → {target refactor/spec path, or "to be planned"}

### P2 (important, next sprint)

- {item}

### P3 (nice to have)

- {item}

---

## Appendix

- **Method**: {how the review was done, what code was read, what tests were run}
- **Gaps**: {modules not reviewed, so readers do not assume full coverage}
