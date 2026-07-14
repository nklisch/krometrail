---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-range-context-query-service
kind: story
stage: implementing
tags: [browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: [epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Build Capture Quality and Event Context Query

## Checkpoint

Add one `TemporalContextQuery` over one already `ResolvedRange`. Derive exact frame availability, bounds, cadence, warnings, declared-gap summary, retention warnings, and persisted capture status/generations from metadata. Query the same sanitized event store in compact focus-aware or verbose cursor mode with exact clipping, filtering, ties, limits, drop/retention warnings, and no wall-time or causal guesses.

## Files

- `crates/krometrail-core/src/timeline/context.rs` (new)
- `crates/krometrail-core/src/timeline/mod.rs`
- `crates/krometrail-core/src/ports/browser_events.rs`
- `crates/krometrail-core/src/{error.rs,lib.rs}`
- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/tests/range_context.rs` (new)

## Acceptance evidence

- Metadata exactly matches resolved frame identity/scope/order; 0/1/many-frame edges, tied times, warning aggregation, and 20,000-frame ceiling are explicit.
- Cadence returns exact min/nearest-rank median/p95/max adjacent session-time deltas; gap duration uses clipped unions and never infers loss.
- Capture status includes the retained sample at/before start plus at most 128 transitions/generations; missing/evicted status warns.
- Optional clips intersect only the resolved retained range; at most 16 focus times must lie inside it; native source/wall clocks never join.
- Compact mode ranks errors/failures, HTTP status, navigation/dialog, then nearest focus distance, with deterministic ties and final chronological presentation.
- Verbose pages (maximum 1,000) use strict scope/time/ordinal/ID cursors without repeats or omissions.
- Collection gaps and retention/corruption unavailable ranges remain visible regardless of class/severity filters and report truncation explicitly.
- Visual epoch counting is absent because artifact generation owns that exact contract.

## Ordering

Depends on the durable v5 source/query adapter. Root integration joins this service with the independently implemented CDP domain authority.