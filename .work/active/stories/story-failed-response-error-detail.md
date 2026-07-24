---
id: story-failed-response-error-detail
kind: story
stage: implementing
tags: [mcp]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Failed responses surface code and recovery in text

## Brief

Failed tool responses drop the structured error from the text surface. In the
2026-07-23 v1.6.1 shakedown a stale `press_keys` reference was internally
classified `stale_reference` (diagnostics show `error_code: stale_reference`)
but the caller-visible text was only:

> press_keys failed: snapshot node has no backing document node

No error code, no recovery, no retry advice — while degraded responses render
their warnings with code/recovery/retry. The structured error object does ride
in `structured_content`, but hosts that render only tool text (the common
agent-facing path) never see it.

Both summary sites format only `error.message`:
`crates/krometrail-mcp/src/response.rs:588` and `:867`
(`format!("{tool} failed: {}", error.message)`).

A second gap: the stale-binding errors constructed in
`crates/krometrail-cdp/src/control/snapshot.rs` (~:995-1010, the `stale(...)`
helper: "snapshot generation is no longer active", "target attachment changed
after the snapshot", "snapshot node has no backing document node") should carry
the canonical recovery guidance ("request a fresh snapshot and retry once with
the new reference") if any of them currently lack it.

## Direction

- Failed-response text includes the stable code and, when present, recovery
  and retry advice, e.g.
  `press_keys failed [stale_reference]: snapshot node has no backing document
  node. Recovery: request a fresh snapshot and retry once with the new
  reference (retry: safe)`.
  One format, both summary sites, batch step failures included
  (response.rs:2604 path) — keep it single-line and calm.
- Ensure the three stale-binding error constructions carry recovery + retry
  consistent with the documented stale_reference contract.
- No change to the structured `error` object shape (no schema churn expected);
  if any published schema text changes, regenerate.

## Acceptance criteria

- [ ] A failed response's text carries `[{code}]` and recovery text when the
      error has recovery; format pinned by test.
- [ ] Stale-binding errors from snapshot reference resolution carry recovery
      and retry advice; pinned by test.
- [ ] Degraded and succeeded summaries unchanged.
- [ ] Full workspace gate green.
