---
id: epic-durable-browser-memory-retention
kind: feature
stage: drafting
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on:
  - epic-durable-browser-memory-segment-format
  - epic-durable-browser-memory-sqlite-index
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Disk-Budget Accounting, Pinning, Eviction, and Session Deletion

## Brief

Own the data-removal capability of the recording store: one user-configured global disk budget applied across the complete Krometrail data directory (active sessions, retained sessions, indexes, browser events, generated artifacts), with segment-granular pinning, oldest-unpinned-first eviction, an explicit paused-budget state when only pinned data remains, and session-scoped deletion. This feature keeps total storage bounded and makes stopped sessions queryable under the same budget without requiring an explicit archive action.

Eviction operates on sealed segments: it computes total current usage, identifies the oldest unpinned segments across all sessions, deletes candidates in chronological order together with their associated index rows and unprotected artifacts, updates usage accounting, and stops when usage is within budget. When no unpinned data can satisfy the budget, the recorder enters a paused-budget state that is reported clearly through the status surface; pinned evidence is never deleted to make room. Session deletion removes every segment, index row, artifact, and event belonging to one session id.

This feature does not own the segment byte format, the SQLite schema, open-segment recovery, or range resolution. It is the runtime data-removal authority that consumes the segment enumeration from the segment-format feature and the removal helpers from the SQLite index feature.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the segment-format and SQLite-index features; produces the status surface that the SPEC requires and the bounded-storage guarantee the product thesis depends on.
- Design decisions inherited: one global budget across the whole data directory; stopped sessions stay queryable under the budget and become eligible for ordinary oldest-first eviction unless pinned; pinning is deliberately segment-granular and protects every segment intersecting the requested range; the all-pinned budget pauses capture rather than deleting protected evidence.

## Simplification opportunity

- Treat session deletion and budget eviction as one cohesive data-removal capability sharing the same primitive helpers (`remove_segment`, `remove_frame_rows`, `remove_artifact`, `update_usage`) supplied by the SQLite-index feature. The two operations differ only in their candidate predicate (oldest-unpinned-across-all-sessions vs all-segments-for-one-session).
- Compute usage from a single authoritative usage-accounting table rather than summing directory sizes on every status query. Eviction updates the table; status reads it.
- The paused-budget state is a small explicit state machine, not a flag tangled into capture. Reporting it through the status surface satisfies the SPEC's "whether eviction or recording is blocked" field.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Disk Budget and Retention (global budget, default 10 GB, time-based immutable segments, oldest-unpinned eviction, pinning, paused-budget behavior, status surface fields), Local Data and Telemetry (deletion by session id removes frames, events, artifacts, indexes)
- `docs/ARCHITECTURE.md` — Retention (eviction loop, paused-budget state, segment-granular pinning), Failure Isolation (an exhausted disk budget pauses capture when protected data prevents eviction)
- `docs/EVALUATION.md` — Storage and Retention Evaluation (every claim this feature must verify)

## Scope and honest non-goals

**In scope:**

- Global budget configuration and validation (non-zero, default 10 GB) applied to the whole data directory.
- Total usage accounting (segments + indexes + browser-event payloads + generated artifacts) maintained as the authority for status queries and eviction decisions.
- Segment-granular pin and unpin of a requested time range: protect (or release) every sealed segment intersecting the range.
- Oldest-unpinned-first eviction: enumerate sealed segments across all sessions, delete candidates in chronological order with their index rows and unprotected artifacts, update usage, stop when within budget.
- Paused-budget state machine: enter when only pinned data could satisfy the budget; do not delete protected evidence; surface the blocked state clearly; resume when space frees.
- Session deletion by session id: remove all segments, frame index rows, capture gaps, interactions, markers, browser events, artifact manifests, and pins for that session.
- The status surface reporting: configured budget, current usage, pinned usage, oldest retained time, newest retained time, capture cadence, recorded and dropped frames, and whether eviction or recording is blocked.

**Non-goals:**

- The segment byte format and writer — owned by `epic-durable-browser-memory-segment-format`.
- The SQLite schema, migrations, and per-row removal helpers — owned by `epic-durable-browser-memory-sqlite-index` (this feature calls those helpers).
- Open-segment recovery and index reconciliation on startup — owned by `epic-durable-browser-memory-recovery`. Retention runs against a recovered, consistent store; it does not reconcile.
- Range resolution and artifact generation — owned by `epic-durable-browser-memory-range-resolution` and the temporal-vision epic respectively. Retention only removes unprotected artifacts whose source data was evicted; it does not generate artifacts.
- Sub-segment or per-frame pin granularity. Initial pinning is deliberately segment-granular per the architecture decision.

## Notes for the design pass

- Eviction must never delete a pinned segment even to satisfy the budget; the paused-budget state is the only legal response when no unpinned candidate exists. Test this directly.
- Artifact removal must respect provenance: an artifact whose source frames were all evicted is removed; an artifact whose source frames survive stays (per the evaluation's "artifacts retain valid provenance or are removed with their source data" check).
- The status surface's "oldest retained time" and "newest retained time" must be derivable from the index, not from a separate retained-range table that can drift.
- Coordinate the open-segment-beyond-budget tolerance with the recovery feature: at most one open segment may exceed the budget while recording, and that bound is reported (foundation evaluation requirement).
- Map exhausted-budget pauses to the existing `ErrorCode::BudgetExhausted` at the boundary; do not invent new error categories here.
