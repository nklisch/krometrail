---
id: epic-a-grade-reliability-artifact-memory-envelope
kind: feature
stage: backlog
tags: [visual, perf, storage, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Account for source materialization in artifact memory limits

## Outcome and priority

Encoded frames are loaded before later artifact memory reservations. Worker reservations alone do not demonstrate that total request peak memory is bounded, particularly across concurrent requests and ranges with unselected epochs.

- **Priority:** P2 — wave 3 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Risk: admission ordering is code-traced; peak-memory breach has not been reproduced.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- src/artifacts/service.rs:107 — frame loading precedes planning/epoch selection
- src/artifacts/service.rs:493,568 — later memory reservations
- crates/krometrail-store/src/recording.rs:1461 — frames_by_id materialization

## Acceptance criteria

- [ ] Measure encoded/decoded buffers, planning structures, in-flight reads, and concurrent-request peak RSS/allocations for cold/warm and multi-epoch ranges.
- [ ] Define the full accounted memory envelope and reserve or bound source materialization before allocation; selected-subset reads preserve complete source provenance.
- [ ] Concurrent large requests stay within the declared envelope or receive bounded, actionable admission outcomes; cancellation releases all charges and source buffers.
- [ ] Regression tests compare declared versus observed accounting, preserve exact source ordering/identity, and cover malformed/oversized metadata and incomplete reads.
- [ ] Document measured non-issues if existing lower-layer limits already bound a suspected path; do not call the source reader unbounded without evidence.

## Implementation direction and boundaries

Prefer metadata-first planning and bounded reads where justified. Coordinate with existing profiling work rather than creating a second benchmark framework.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `perf-scout-profile-artifact-stages` — related authority/context, not an implicit blocking dependency.
- `perf-scout-bounded-parallel-decode` — related authority/context, not an implicit blocking dependency.
