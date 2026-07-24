---
id: story-failed-response-error-detail
kind: story
stage: done
tags: [mcp]
parent: null
depends_on: []
release_binding: 1.6.2
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

## Implementation notes

- Execution capability: inline implementation; the summary formatter and stale-reference contract are localized to the MCP/CDP error boundaries.
- Review weight: standard default; no independent review requested.
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-core/src/error.rs`, `crates/krometrail-cdp/tests/temporal_evidence.rs`, and `crates/krometrail-cdp/src/session/reconnect.rs`.
- Tests added/updated: `failed_summary_includes_code_recovery_and_retry_on_one_line` pins the single-line text format; `visible_errors_are_structured_without_json_text_duplication`, batch summary assertions, and stale-reference recovery assertions were updated to the current contract.
- Simplification: both failed-response summary sites and batch step failures now use one formatter that exposes the stable code, optional recovery, and retry advice without changing structured error fields.
- Discrepancies from design: stale-reference recovery was aligned centrally with the fresh-snapshot wording across the existing stale-reference boundary, so related current-geometry and reconnect assertions use the same contract.
- Adjacent issues parked: none.

## Review

Bounded fresh-context review: PASS, no findings. Single formatter confirmed at both summary sites and batch steps with no recovery double-render; degraded/succeeded summaries unchanged; stale-binding recovery aligned across boundaries.
