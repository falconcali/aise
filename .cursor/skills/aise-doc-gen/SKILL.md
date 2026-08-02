---
name: npc-doc-gen
description: Generate AISE project documentation (design / refactor / spec / review / issue) from the current conversation, with a unified filename convention, correct directory placement, and type-specific structural requirements. Use when the user asks to turn the current discussion into an AISE doc, e.g. "write a design doc", "turn this into a refactor doc", "start a spec for this plan", "write up a review", "log this as an issue", or "put this discussion under doc/design".
---

# npc-doc-gen — AISE documentation generator

Turn the current conversation into a well-formed document under the AISE
repo's `doc/` tree. The core value is consistent naming, correct directory,
and the right structure for each document type.

---

## 1. Triggers

Activate when the user wants to:

- "write a design / refactor / spec / review doc"
- "turn this discussion into a doc under `doc/design` (or `doc/refactor`, `doc/exec`, `doc/review`, `doc/issue`)"
- "summarize what we discussed into a document"

If the user says "write a doc" without naming a type, ask for the type first
(use `AskQuestion` or a direct question).

---

## 2. Workflow (run in order)

```
[1] Determine doc type
      │
      ▼
[2] Determine model alias (the model you are running as)
      │
      ▼
[3] Get today's date from the shell
      │
      ▼
[4] Pick a kebab-case topic
      │
      ▼
[5] Assemble filename + target directory
      │
      ▼
[6] Read the matching template → fill from the conversation → write
      │
      ▼
[7] Return the full file path to the user
```

### Step 1 — Type

| User intent | Type | Directory |
|---|---|---|
| Explore an idea, weigh trade-offs, compare options | `design` | `doc/design/` |
| State what to change, how, and the migration strategy | `refactor` | `doc/refactor/` |
| Drive AI code generation; only what to do / not do | `spec` | `doc/exec/` |
| Review code / architecture / project and give a verdict | `review` | `doc/review/{subdomain}/` (e.g. `architecture-review/`, `project-review/`) |
| Record a concrete problem or feedback | `issue` | `doc/issue/` |

When the type is unclear, ask. Do not invent new type directories.

### Step 2 — Model alias

Use the model running **this** conversation (read it from the system info):

| Model family | Alias |
|---|---|
| Claude Opus (any version) | `opus` |
| Claude Sonnet (any version) | `sonnet` |
| Claude Haiku | `haiku` |
| Other Claude | `claude` |
| GPT family | `gpt` |
| Gemini | `gemini` |
| Anything else | the model's short name, lowercase |

Always lowercase. A version suffix (`v1` / `v2`) is optional, not required.

### Step 3 — Date

Use today's date, format `YYYY-MM-DD`. Get it from the shell, do not hardcode:

```powershell
Get-Date -Format yyyy-MM-dd
```

### Step 4 — kebab-case topic

- Distill an English phrase that recovers the topic in one glance, ≤ 6 words.
- All lowercase, joined by `-`.
- Examples: `message-pipeline-refactor`, `director-intent-design`, `world-ctx-channel-split`.
- For a phased spec, append `-phase-N` (e.g. `-phase-0`).

Never put non-ASCII characters in the filename; the topic always stays English
kebab-case even if the document body is in another language.

### Step 5 — Filename

Format:

```
{YYYY-MM-DD}-{kebab-topic}-{type}[-{suffix}]-{model}[-{version}].md
```

- Required: date, topic, type, model.
- Optional: `suffix` (e.g. `phase-0`, `compliance`), `version` (`v1`, `v2`).

Examples:

- `2026-04-21-cross-world-sync-design-opus.md`
- `2026-04-21-director-intent-refactor-opus.md`
- `2026-04-21-director-intent-spec-opus.md`
- `2026-04-21-skill-discovery-review-opus.md`

Join with the directory from Step 1 and write to `<repo-root>/doc/...`. The repo
root is the AISE repository root; infer it from the shell working directory
or use the path the user gave.

**Before writing**: if the target file already exists, do not overwrite. Append
`-v2` (then `-v3`, …) or confirm with the user.

### Step 6 — Fill the template

Read the matching template and fill it with what was **actually discussed** in
the conversation:

- design → [`templates/design.md`](templates/design.md)
- refactor → [`templates/refactor.md`](templates/refactor.md)
- spec → [`templates/spec.md`](templates/spec.md)
- review → [`templates/review.md`](templates/review.md)
- issue → [`templates/issue.md`](templates/issue.md)

Placeholders are wrapped in `{…}` — replace all of them. Template sections are
the minimum required set; add subsections when needed, but do not drop required
ones.

### Step 7 — Deliver

After writing, give the user:

1. The full absolute path.
2. One sentence: doc type, model, topic, and the key sections.
3. If anything is left as `TBD`, call it out explicitly so the user can fill it.

---

## 3. Type-specific requirements

Shared baseline is in §4; below are only the constraints unique to each type.

### design

- Must cover three things:
  1. **Context**: current state, the concrete problem, why now.
  2. **Options & trade-offs**: at least 2 candidates with pros/cons and the
     reason for the choice (if there is genuinely one option, still say why not
     X / Y).
  3. **Design**: target structure, core types / functions / data flow, key
     decisions.
- No implementation-level code (that belongs to the spec or the code itself).
- No bare "what to do" list without reasons.

### refactor

- Must cover:
  1. **Context**: where the current architecture is broken.
  2. **Refactor principles**: hard refactor vs compatibility; whether any
     fallback is kept; the delete-old-path strategy.
  3. **Change list**: a table of every affected file / module / type, with
     priority or phase.
  4. **Target structure** and **migration steps**.
  5. **External impact**: URLs, APIs, DB schema, prompts, config files.
- AISE defaults to hard refactors (no fallback, no dual paths — see
  `R-REFACTOR-01/02` in `AGENTS.md`). Default to that; if you keep
  compatibility, state the reason explicitly.

### spec

- Written in **English** (it feeds AI code generation).
- Must contain:
  1. `## 1. Goal` — one sentence.
  2. `## 2. Scope & Non-Goals` — the **Non-Goals subsection is required**.
  3. `## 3. Contracts` — concrete type / function signatures, event / protocol
     shapes; no prose-only descriptions.
  4. `## 4. Behavior Rules` — numbered, testable rules; error handling,
     concurrency, observability.
  5. `## 5. Acceptance Criteria` — a mechanically verifiable checklist.
- No design discussion (link back to the source design instead).
- No vague wording ("properly handle errors", "should be robust" — these are
  failed specs).
- The header must carry `> Source Design: [link]`. If there is no source
  design, the spec does not stand — write the design first.
- **Multi-file specs**: when one spec splits into several files (e.g. phases),
  group them in a subfolder named after the topic plus the `-spec` suffix:
  `doc/exec/{kebab-topic}-spec/` (e.g. `doc/exec/gateway-service-spec/`,
  `doc/exec/cognition-slow-brain-spec/`). Each file keeps the full filename
  (date, topic, type, phase suffix, model). A single-file spec stays directly
  under `doc/exec/` with no subfolder.

### review

- Must contain:
  1. Header: review target, date, reviewer (`{full model name}`, e.g.
     `Opus (Claude)`), review standard.
  2. `## Overall Verdict` — 3–5 lines: conclusion first, then reasons.
  3. `## Red Lines` — a table of must-fix problems.
  4. `## Per-Dimension Analysis` — by the review standard's dimensions, or by
     module / subsystem.
  5. `## Recommendations` — prioritized P1 / P2 / P3.

### issue

- Must contain:
  1. `## Symptom`: what the user sees, how the system behaves.
  2. `## Reproduction`: steps / input / environment.
  3. `## Impact`: which features, which users, severity.
  4. `## Proposed Fix`: short-term workaround + long-term direction (if known).

---

## 4. Quality baseline (all docs)

1. **Concise**: every sentence carries information. Cut filler. No adjective
   stacking.
2. **Accurate**: wrap code / paths / type / function names in backticks; mark
   anything uncertain as `TBD`; never fabricate.
3. **Clear**: section titles are semantically complete; order follows
   **Why → What → How**.
4. **AI-readable**:
   - Structured: headings / lists / tables / Mermaid.
   - Stable terms: one concept, one name, self-consistent within the doc.
   - Local references use relative paths, e.g. `[link](../../src/core/...)`.
   - Cite source lines as `path:line`.
   - Avoid marketing words ("extremely powerful", "perfect", "completely
     solves").
5. **No emoji** unless the user asks.
6. **Mermaid** is for showing multi-component relationships, not decoration.
7. **Markdown basics**: heading levels start at H1 and stay continuous; code
   blocks declare their language.

---

## 5. Common mistakes

- Do not hardcode the date — fetch it from the shell each time; sample dates in
  this skill are only examples.
- Do not invent discussion content — leave `TBD` where the conversation did not
  settle something.
- Do not widen scope — design means design (no spec), spec means spec (no
  design).
- Do not push design discussion into a spec — a spec only states what to
  do / not do.
- Do not use Windows-style paths in docs — use forward slashes,
  `doc/design/xxx.md`.
- Check for name clashes before writing — if a same-date same-topic doc exists,
  append `-v2` or confirm with the user.

---

## 6. Template index

- [`templates/design.md`](templates/design.md) — design doc skeleton
- [`templates/refactor.md`](templates/refactor.md) — refactor doc skeleton
- [`templates/spec.md`](templates/spec.md) — English spec skeleton
- [`templates/review.md`](templates/review.md) — review doc skeleton
- [`templates/issue.md`](templates/issue.md) — issue doc skeleton
