---
id: bug-recover-browser-event-collection-status
kind: story
stage: done
tags: [bug, browser, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: review
gate_finding: browser-event-context-I1
created: 2026-07-14
updated: 2026-07-14
---

# Recover browser-event status after persistence resumes

## Symptom

A single transient browser-event sink failure permanently leaves `BrowserEventCollectionStatus` at `Failed`, even after the writer successfully persists the coalesced collection gap and resumes event batches. Later collection-state evidence therefore reports a failed collector while durable event flow is operational.

## Root cause

`writer_loop` calls `PipelineInner::mark_failed` on every failed batch or gap flush. Success resets retry backoff and counters but never restores status. `mark_operational` runs only during target restoration, so status recovery incorrectly depends on reconnect rather than writer health.

## Fix approach

After a successful gap flush or normal batch persist, recover status only when the current state is `Failed`: use `Operational` when no event classes are unavailable and `Degraded` when optional source classes remain unavailable. Do not overwrite `Suspended`, `Stopped`, `Disabled`, or ordinary restore-owned state.

## Regression test

The existing fail-once sink test now requires `Operational` after the collection-gap event is durably observed; it failed before the fix with `Failed`. A focused case proves recovery retains `Degraded` when an optional event class remains unavailable. Shutdown-deadline exhaustion continues ending in `Failed`.

## Implementation notes

- Execution capability: baseline inline ownership; one status transition repeated at two writer success points.
- `PipelineInner::mark_recovered` changes only a current `Failed` state, choosing `Operational` for no unavailable classes or `Degraded` otherwise. It cannot overwrite disabled, suspended, stopped, starting, or already healthy states.
- Successful gap flushes and normal batches invoke recovery after durable append and persisted-count update. Failure/backoff/gap evidence and shutdown semantics are unchanged.
- The focused test reproduced the stale `Failed` state before the fix, then all six pipeline tests passed after it. Locked Rust 1.85 full workspace tests, format, and Clippy with warnings denied passed.
- Parked separately: `idea-temporal-context-clip-and-truncation-exactness` and `idea-redact-windows-drive-relative-paths` capture accepted lower-risk review nits.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Evidence**: Bounded standalone-story review inspected commit `9ff5d28`, confirmed recovery occurs only after successful durable writes and only from `Failed`, preserves optional-source degradation, and cannot overwrite lifecycle-owned statuses. Failure gap/backoff, counters, shutdown deadline failure, and source availability sets remain unchanged. The reproducing regression and full Rust 1.85 workspace gate passed. No independent reviewer ran, as required for a standalone fix story.
