---
id: story-batch-timeout-preserves-dispatched-record
kind: story
stage: review
tags: [browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Batch timeout preserves a dispatched step's record

Promoted from `idea-bundle-latest-interaction-anchor`-era backlog capture
`idea-batch-timeout-preserves-dispatched-record` (surfaced by the
postcondition-core cross-model review, finding 3 residual).

## Defect

`dispatch_bounded` (`crates/krometrail-cdp/src/control/batch.rs` ~368) races
the whole `execute_operation` future against `sleep_until(deadline)` and DROPS
it on timeout. A deadline that fires after input dispatch but before record
persistence erases the proven-dispatch evidence: no `InteractionRecord` is
constructed or persisted, and the step reports a bare timeout. This
contradicts SPEC's principle that observation failure after a proven action
must not erase the dispatch or imply replay is safe.

## Design direction

Cooperative post-dispatch budgeting, external kill only as a backstop:

1. `OperationExecutionContext` already carries `deadline` (batch.rs ~122).
   In the interaction execution path, once input has been dispatched, clamp
   every post-dispatch phase (action completion wait, compositor rendezvous,
   postcondition probes, live observation, side-channel reconciliation) to
   the remaining budget derived from that deadline. On exhaustion, produce
   the interaction result with degraded/unavailable observation parts — the
   existing `unavailable_observation`/degraded machinery — so the record is
   constructed, enriched as far as facts were observed, and persisted, and
   the step returns a Completed result whose degraded evidence names the
   budget exhaustion.
2. Keep the hard `select!` kill as a backstop at `deadline + grace` (bounded,
   e.g. 500 ms) so a truly wedged pre-dispatch transport still times out
   cleanly; pre-dispatch timeouts keep today's behavior (no record — nothing
   was dispatched, `wait_timeout_error`).
3. No batch wire-contract change; standalone (non-batch) operations without a
   context deadline are untouched.

## Acceptance

- Deterministic double: a batch step whose post-dispatch observation stalls
  past the deadline returns a step result carrying the interaction anchor,
  with degraded observation evidence, and the record is persisted (visible
  via the store/timeline seam) — while the batch still terminates as timed
  out per its existing termination rules.
- Deterministic double: a step stalled BEFORE dispatch (e.g. resolution or
  actionability probe wedged) still yields the current timeout with no
  record.
- The backstop fires when the cooperative path itself is wedged (fault
  injection past dispatch that also ignores budgets).
- Existing batch, wait-step, and timeout tests unchanged in meaning; full
  workspace gate green.

## Implementation

- Root cause confirmed: `dispatch_bounded` dropped the entire operation future
  at the batch deadline, so a step that had already dispatched input could
  lose its interaction record before enrichment and persistence completed.
- Landed cooperative post-dispatch budgeting through the existing execution
  context. Completion waits, compositor rendezvous, postcondition probes,
  live observation, and the cancellable side-channel inventory pull now clamp
  to the remaining batch deadline and use the existing unavailable observation
  shapes on exhaustion. The batch retains a 500 ms external backstop and
  still reports `timed_out` when a completed step crosses its deadline.
- Preserved pre-dispatch timeout behavior and standalone operations without a
  deadline. No wire-contract or new error family was introduced; budget
  exhaustion uses the existing observation degradation code with a bounded
  cause. The non-cancellable target-reconciliation reducer remains outside
  the timeout wrapper, protected by the existing backstop.
- Added deterministic dispatched, pre-dispatch, and wedged-persistence tests;
  updated the current Batching specification to describe the preserved
  record behavior.
- Verification: formatting, wire-schema checks, workspace check, workspace
  tests, and workspace clippy all passed. The full workspace test gate needed
  loopback socket permission for existing endpoint/session tests; with that
  permission it passed completely.
