---
id: epic-durable-browser-memory-range-resolution-qualification-handoff
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-range-resolution
depends_on: [epic-durable-browser-memory-range-resolution-resolver-semantics]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Qualify range resolution and record sibling handoffs

## Checkpoint

Finish the feature with integrated resolver qualification and honest handoff notes for sibling-owned record writers. Do not add structured interaction, navigation, marker, browser-event, pin, or artifact rows in this feature.

## Required handoffs

- Browser page lifecycle and verified interactions must persist the existing `InteractionAnchor` projection (`interaction_id`, `session_id`, `target_id`, operation, and timing) before interaction/window/recent anchors can resolve successfully.
- Navigation-producing browser features must mint `NavigationId` and append a typed `ObservationKind::Navigation` timeline observation at the accepted navigation commit point.
- Marker-producing features must mint `MarkerId` and append a typed `ObservationKind::Marker` timeline observation at the declared session time.
- Retention owns stronger evicted-vs-never metadata, pin protection, and deletion policy; this resolver consumes retained bounds/warnings only.

## Acceptance evidence

- [x] One end-to-end SQLite test writes catalog rows, frames, a capture gap, marker/navigation timeline observations, and resolves explicit time, wall-clock, marker, navigation, source-frame, and currently-absent interaction anchors with expected results.
- [x] Boundary tests cover zero-length ranges, endpoint inclusivity, timestamp ties, `u64::MAX` overflow, wall times before session start, target with no frames, and gaps at exact boundaries.
- [x] Wrong-target/wrong-session tests cover source frames, marker/navigation observations, and interactions.
- [x] Implementation summary documents any unresolved sibling dependency instead of claiming unsupported anchors work.
- [x] Workspace format, locked check, locked tests, and Clippy are green or unrelated concurrent-owner failures are precisely identified.

## Implementation and handoff

The integrated qualification suite is `crates/krometrail-store/tests/range_resolution.rs`. It verifies inclusive frame and ordinal ordering, equivalent wall-clock/session ranges, generic marker/navigation anchors, gap include/reject, strict/partial retention, empty-target and pre-session failures, checked `u64::MAX` window overflow, and source/marker scope errors.

Interaction and latest-interaction requests intentionally return `NotFound` from `SqliteIndex` until browser-operation features persist the existing `InteractionAnchor` projection. Navigation owners must append typed `ObservationKind::Navigation` rows; marker owners must append typed `ObservationKind::Marker` rows. Retention remains authoritative for eviction and pin state; this resolver only reports retained-frame bounds and warnings.

Verification: `cargo fmt --all -- --check`; `cargo check --workspace --all-targets --locked`; `cargo test --workspace --all-targets --locked` (379 passed); and `cargo clippy --workspace --all-targets --locked -- -D warnings` all pass. Focused range tests pass (8 tests).
