---
id: epic-durable-browser-memory
kind: epic
stage: implementing
tags: [storage, browser]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-13
---

# Durable Browser Memory

## Brief

This epic turns the validated live browser stream into durable, time-indexed memory. It owns immutable frame segments, searchable metadata, interactions and markers, browser-event evidence, explicit gaps, range resolution, disk-budget accounting, segment-granular pinning, chronological eviction, and crash recovery.

The recording store preserves compressed source frames without transcoding during ingestion and keeps session duration independent from memory use. It exposes enough operational evidence to tell an agent when frames were dropped, a target was hidden, retention removed data, or protected evidence paused capture.

This epic does not render temporal visual artifacts or expose the complete agent investigation workflow. It provides the reliable retained ranges and source references those consumers require.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Action Timeline, Browser Events, Disk Budget and Retention, and Temporal Ranges
- `docs/ARCHITECTURE.md` — Recording Store, Retention, Crash Recovery, and Temporal Range Resolution
- `docs/EVALUATION.md` — Storage and Retention Evaluation

## Design decisions

- **Disk-budget scope:** Apply one user-configured budget to the complete Krometrail data directory across active sessions, retained sessions, indexes, browser events, and generated artifacts. Eviction selects the oldest unpinned segments globally so total storage remains bounded.
- **Stopped-session retention:** Keep stopped sessions queryable under the global budget. They require no explicit archive action and become eligible for ordinary oldest-first eviction unless a range is pinned.
- **Storage port shape — focused slices, not a god-port:** The existing `RecordingSink` (append_frame / append_gap / flush) and `TimelineStore` (append / range) ports are too thin for retention, recovery, range resolution, and deletion. Each child feature extends `krometrail-core`'s storage boundary with the focused port slice it needs (a frame-source read port, a retention port, a recovery/open hook) rather than collapsing everything into one monolithic trait. Keeps the domain infrastructure-free and lets adapters compose; reversible by later consolidation.
- **Range resolver ownership — this epic owns it; the temporal-query epic consumes it:** The architecture places one range resolver in the data path but does not assign it to a crate. This epic owns the resolver and the `ResolvedRange` core type because resolution depends on the storage indexes (frame / interaction / navigation / marker / gap lookups) and produces addresses into the store. The sibling `epic-temporal-debugging-workflow` epic consumes `ResolvedRange` for artifact generation.
- **Generic timeline index + structured tables only for core-defined records:** Every `TimelineObservation` row is persisted through one generic index keyed by `(session, target, session_time, kind, payload_ref)`, driven from the existing `ObservationKind` registry, instead of one table per observation kind. Dedicated structured tables are added only for records `krometrail-core` already defines (`CaptureGap` today). Richer structured records (`InteractionRecord`, browser-event payloads) arrive when sibling epics define those types; this epic exposes the persistence surface but does not invent types it does not own.
- **Recovery-before-retention startup ordering; pins trusted across recovery:** Store open runs recover-then-retain. Pins live in SQLite (WAL-durable) and are trusted across recovery; recovery reconciles frame-index rows and usage against actual segment contents only. Reconstructing pins would duplicate authority between SQLite and the segment files.
- **Shared index-mutation helpers, two consumers:** The primitive segment/index row-removal and usage-update helpers live with the SQLite-index feature. Retention (oldest-unpinned eviction, session deletion) and recovery (orphan-row reconciliation) compose those primitives with different predicates rather than duplicating index-mutation logic.

## Decomposition

The epic is split into five capabilities-shaped features aligned with the `krometrail-store` module boundaries (`segments/`, `index/`, `retention/`, `recovery/`) plus the cross-cutting range resolver. The dependency chain is a shallow diamond: the segment format publishes the frame-address contract and the recoverable-record layout that the SQLite index consumes, then retention, recovery, and range-resolution parallelize against the index. Session deletion is grouped with retention because both are runtime data-removal operations sharing the same primitive helpers; recovery stays separate because it is a startup-time consistency pass in the opposite direction. The original "interaction, marker, capture-gap, and browser-event persistence" sketch collapses into the SQLite-index feature because all observation kinds are persisted through one generic timeline index, with structured tables layered only for core-defined records.

### Child features

- `epic-durable-browser-memory-segment-format` — versioned append-only frame segment format, sealed-footer immutability, bounded rotation, the `(segment_id, byte_offset)` frame-address contract, and the frame-write half of the recording sink — depends on: `[]`
- `epic-durable-browser-memory-sqlite-index` — versioned SQLite schema in WAL mode, generic timeline observation index, structured `CaptureGap` table, frame-source read port, `TimelineStore` adapter, and the shared index-mutation helpers — depends on: `[epic-durable-browser-memory-segment-format]`
- `epic-durable-browser-memory-retention` — global disk-budget accounting, segment-granular pinning, oldest-unpinned-first eviction, paused-budget state, session deletion, and the status surface — depends on: `[epic-durable-browser-memory-segment-format, epic-durable-browser-memory-sqlite-index]`
- `epic-durable-browser-memory-recovery` — open-segment scanning, complete-record recovery, trailing truncation, sealing, and frame-index plus usage reconciliation on startup — depends on: `[epic-durable-browser-memory-segment-format, epic-durable-browser-memory-sqlite-index]`
- `epic-durable-browser-memory-range-resolution` — the single temporal range resolver, the `ResolvedRange` core type, every SPEC natural anchor, implicit-interaction windows, and clear evicted / never-captured / wrong-target / gapped failure modes — depends on: `[epic-durable-browser-memory-sqlite-index]`

### Simplification arcs

- Replace the in-memory `FakeRecording` / `FakeTimeline` test doubles' assumed surfaces with the real segment-write and SQLite adapters wired through the composition root, so no production path depends on a test fake.
- Persist all `ObservationKind` values through one generic timeline index driven from the existing `krometrail-core` registry, eliminating one-table-per-kind duplication and the hand-maintenance that would require.
- Publish the `(segment_id, byte_offset)` frame-address contract and the `ResolvedRange` contract once each in `krometrail-core`, so the index, retention, recovery, and the sibling temporal-query epic never re-derive them.
- Compute usage from a single authoritative usage-accounting table maintained by eviction and deletion, rather than summing directory sizes on every status query.

### Decomposition risks

- **Segment-format ↔ index coupling on the frame-address contract.** The `(segment_id, byte_offset)` addressing model is shared by every consumer. If segment-format evolves the addressing after sqlite-index lands, migrations ripple. Mitigation: segment-format must publish the addressing contract as a stable core surface in its first design pass; consumers import it.
- **Shared removal helpers under divergent predicates.** Retention (oldest-unpinned-across-all-sessions) and recovery (orphaned-tail-frames) both call the index-mutation primitives. If their needs diverge, the helpers risk bloating into per-caller branches. Mitigation: keep the primitives minimal (`remove_segment`, `remove_frame_rows`, `update_usage`) and let each caller compose.
- **`ResolvedRange` contract drift with the temporal-query epic.** `ResolvedRange` is the load-bearing contract with `epic-temporal-debugging-workflow`. Field drift blocks the consumer. Mitigation: settle the type in `krometrail-core` once here; the consumer imports rather than re-declares.
- **Crash-recovery write-order invariant.** "Metadata does not claim a frame exists until its complete segment record is durable" requires segment bytes to be durable before the index row commits. A crash between the two must always leave an orphan index row recoverable by removal, never a dangling payload. Mitigation: recovery owns a real crash-mid-write integration test that crosses the segment-format and sqlite-index features.
- **All-pinned budget pause correctness.** Pausing capture without deleting pinned evidence, then resuming when space frees, is subtle and is a SPEC requirement. Mitigation: retention owns the paused-budget state machine and its tests, and reports the blocked state through the status surface.
- **Sibling-epic dependency on this epic's ports.** `epic-agent-browser-operation` (interaction records, browser events) and `epic-temporal-debugging-workflow` (artifact manifests, range reads) consume the storage ports defined here. Wrong port shapes block them. Mitigation: keep ports minimal and capability-focused; do not over-fit to one consumer.
- **Open-segment-beyond-budget tolerance.** The evaluation tolerates at most one open segment beyond budget while recording. Recovery and retention must agree on the bound and report it consistently. Mitigation: the bound is a reported measurement owned by retention, consumed unchanged by recovery's status report.
