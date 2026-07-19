---
id: feature-actionable-failure-surface
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: [feature-temporal-range-artifact-economy]
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Make every failure name its limit and its cheapest recovery

## Brief

Two live shakedowns (2026-07-19) found that failure messages consistently omit
the facts an agent needs to act:

- **Limit errors never state the limit.** Observed: "source read limits exceed
  runtime ceilings", "selected source frame count exceeds the request limit",
  "no exact integer analysis scale fits configured limits", "normalization
  result exceeds configured processing limits" — none carry the ceiling, the
  offending value, or a fitting suggestion. The caller resorts to bisection.
  Contract: every limit rejection reports `(actual, limit)` and, where cheaply
  computable, a suggested value that would succeed.
- **Recovery labels drift from reality.** Observed: `retry: never` +
  `recovery: null` on the CSS-size observation failure that a cross-origin
  navigation fully recovers; "retry once ... safe" on a deterministic node-cap
  failure; "narrow the query to a smaller document" on a scroll (no query
  exists). Audit every failure site: `retry` and `recovery` must reflect the
  cheapest real state change that makes the operation succeed, or honestly say
  none exists.
- **Hard failures drop their evidence.** Failed clicks returned a bare error
  string with no `diagnostics.correlation_id` and no interaction context, so the
  failure cannot be temporally anchored or investigated. Degraded responses keep
  full context; hard errors must too.
- **The evaluate_page refusal classifier never fires on current Chrome.**
  `evaluation.rs` matches the needle "side effect" (space), but Chrome 149 emits
  "Possible side-effect in debug-evaluate" (hyphen), so every refusal presents
  as "page evaluation threw: EvalError ...". Match both forms (or normalize
  hyphens) and add a decode test using the real Chrome 149 description string.

Absorbed backlog: `idea-evaluate-refusal-needle-drift`. Depends on
`feature-temporal-range-artifact-economy` because the limit/recovery audit
rewrites messages in the temporal paths that feature restructures. Implementation
via peeragent Codex `gpt-5.6-luna` per operator decision (2026-07-19).

## Simplification opportunity

A shared bounded-limit-error constructor (actual, limit, suggestion) can replace
the current ad-hoc message formatting at each ceiling; check for existing
`operation_error` helpers to extend instead of adding a parallel path. The
recovery audit may delete recovery prose that restates the error rather than
naming an action.
