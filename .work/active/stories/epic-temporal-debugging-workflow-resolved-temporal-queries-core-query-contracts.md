---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-core-query-contracts
kind: story
stage: done
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define the resolved temporal query boundary

## Checkpoint

Add the application-facing `TemporalQueryRequest`/`TemporalQuery` service over the existing `TemporalRangeResolver`, validate natural-anchor windows at constructor and Serde boundaries, and add metadata-only frame availability required for honest retention classification. Keep `TemporalRangeAnchor` and `ResolvedRange` as the only request-anchor and resolved-range authorities.

## Required contract

- `TemporalQueryRequest` directly contains `TemporalRangeAnchor`, `RetentionPolicy`, and `CaptureGapPolicy`; strict/default behavior is complete retention plus included declared gaps.
- Omitted interaction/latest-interaction windows resolve with the existing 150 ms pre-start and 250 ms post-observation/completion policy.
- Explicit interaction/navigation/marker windows serialize as whole `before_ms`/`after_ms` values and reject either side above 120 seconds.
- `FrameSource` exposes metadata-in-time-range, metadata-in-ordinal-range, and `FrameAvailability { retained_bounds, evicted_ranges }` reads; resolver frame identity selection does not read encoded payloads.
- Availability semantics distinguish eviction from never-captured evidence, accept only explicitly allowed contiguous edge trimming, and reject internal eviction holes.
- Add the inward-facing `InteractionEvidenceSink` and `InteractionRecordSource` ports using existing interaction types rather than a persistence copy.

## Acceptance evidence

- [ ] Constructor/Serde coverage exercises all seven anchor forms, malformed scope/ranges, unknown fields, policies, whole-millisecond limits, and implicit options.
- [ ] Resolver tests cover complete, edge-evicted, internally evicted, never-captured, wrong-target, and gap include/reject behavior.
- [ ] Frame IDs retain capture-ordinal order and related IDs retain deterministic timeline order.
- [ ] Core source guards prove all new contracts remain infrastructure/runtime neutral.

## Ordering

This establishes the signatures and semantics consumed by the SQLite, CDP, and composition checkpoints. It has no sibling dependency and must land first.

## Implementation notes

- Execution capability: highest; the caller selected this tier for cross-cutting temporal and persistence risk. Direct-read only, with all five checkpoints retained under one feature owner.
- Review weight: standard, from the autopilot caller; child checkpoint review is not applicable.
- Files changed: `crates/krometrail-core/src/{lib.rs,ports/{frames.rs,mod.rs,range.rs},timeline/{mod.rs,query.rs,range.rs}}`, plus the required `FrameSource` adapter signatures in `crates/krometrail-store/src/index/frames.rs` and one existing constructor call in `crates/krometrail-store/tests/range_resolution.rs`.
- Tests added/updated: validated request/anchor/window round trips, unknown-field and malformed-scope rejection, nil source-frame rejection, exact implicit 150 ms/250 ms options, and whole-millisecond wire coverage. Existing overflow coverage now uses the bounded whole-millisecond constructor.
- Semantics delivered: one `TemporalQueryRequest`/`TemporalQueryService`; object-safe evidence/read ports; `FrameAvailability`; metadata-only resolver reads; exact complete-range preservation inside retained bounds; explicit contiguous edge eviction only; internal-hole, uncaptured, scope, and gap failures; deterministic capture-ordinal and timeline ordering.
- Simplification: retained `TemporalRangeAnchor`, `TemporalRangeResolver`, and `ResolvedRange` as the only authorities; removed encoded-segment reads from range resolution instead of adding a parallel query model.
- Discrepancies from design: the SQLite metadata methods were added mechanically with empty eviction memory in this checkpoint so the workspace remained compilable; v3 tombstone reads replace that temporary empty adapter in the immediately dependent durable-index checkpoint. No external contract changed.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; `cargo check -p krometrail-store --all-targets --locked`; `cargo test -p krometrail-core --all-targets --locked` (74 passed); `cargo clippy -p krometrail-core --all-targets --locked -- -D warnings`.
