---
id: epic-durable-browser-memory-sqlite-index
kind: feature
stage: drafting
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on: [epic-durable-browser-memory-segment-format]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# SQLite Metadata Index and Timeline Indexing

## Brief

Own the searchable metadata layer of the recording store: a versioned SQLite schema running in write-ahead logging mode that makes recorded data queryable across active and stopped sessions. The index is the single metadata authority — sessions, targets, frame addresses, segment registrations, capture gaps, interactions, markers, browser events, pins, artifact manifests, and usage accounting — and it implements the existing `TimelineStore` port plus the structured-persistence surface for records core already defines.

Timeline observations are persisted generically: one index over every `ObservationKind` (frame, interaction boundary, navigation, target lifecycle, visibility change, capture gap, console message, JavaScript exception, network lifecycle, marker) rather than ten parallel tables. Structured per-kind tables are added only for records `krometrail-core` already defines today (`CaptureGap`), with explicit extension-point ports for richer structured records (`InteractionRecord`, browser-event payloads) that arrive when sibling epics define those types.

This feature does not own the segment byte format, budget accounting, eviction, recovery reconciliation, or range resolution. It is the metadata authority that retention, recovery, and range-resolution read from and mutate.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: foundational metadata feature — depends on the segment-format feature for the `(segment_id, byte_offset)` frame-address contract; consumed by retention, recovery, and range-resolution.
- Design decisions inherited: timeline observations are indexed generically by kind; structured tables exist only for core-defined records; the index is the single searchable metadata surface; ports are extended in focused slices (a frame-source read port is added here, alongside the existing `TimelineStore` write/range port).

## Simplification opportunity

- Persist all `ObservationKind` values through one generic timeline index keyed by `(session, target, session_time, kind, payload_ref)` rather than maintaining one table per observation kind. The discriminator and payload-ref columns are enough for range queries; structured detail tables layer on top only where core defines a structured record.
- Drive table membership for observation kinds from the existing `ObservationKind` registry in `krometrail-core` so adding a kind does not require hand-editing the schema in two places.
- Replace the in-memory `FakeTimeline` test double's assumed surface with the real adapter wired through the composition root.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Sessions and Targets (session metadata fields), Action Timeline (interaction record fields), Browser Events (recorded evidence and default redaction), Disk Budget and Retention (status surface fields), Temporal Ranges (range query inputs)
- `docs/ARCHITECTURE.md` — Recording Store (SQLite contents and WAL), Domain Model (identifier contracts, observation kinds), Failure Isolation (an SQLite failure stops persistence before accepting unsupported writes)
- `docs/EVALUATION.md` — Storage and Retention Evaluation (status reports the correct retained range; deletion removes all data belonging to the selected session)

## Scope and honest non-goals

**In scope:**

- A versioned SQLite schema with migrations, running in WAL mode, covering: sessions, targets, frame index (frame id, session, target, segment id, byte offset, session time, source/observed times, capture ordinal, format, dimensions, warnings), segment registrations, capture gaps, pins, artifact manifests, usage accounting, and the generic timeline observation index.
- The `TimelineStore` adapter (append + range) backed by SQLite, replacing the test-only fake.
- A frame-source read port in core (read frames by id and by resolved range) plus its SQLite implementation, consuming the `(segment_id, byte_offset)` address from the segment-format feature.
- Structured persistence for `CaptureGap` (a core type today) with range queryability.
- Extension-point ports — but no invented types — for richer interaction records and browser-event payloads that sibling epics (`epic-agent-browser-operation`) will define.
- Schema migrations are forward-only and versioned; a failed migration prevents startup with the failing value identified.

**Non-goals:**

- The segment byte format and writer — owned by `epic-durable-browser-memory-segment-format`.
- Budget accounting, pinning logic, eviction, and session deletion — owned by `epic-durable-browser-memory-retention` (this feature provides the tables and update helpers they call).
- Open-segment recovery and reconciliation — owned by `epic-durable-browser-memory-recovery` (this feature provides the frame-index and usage rows reconciliation targets).
- Natural-anchor range resolution — owned by `epic-durable-browser-memory-range-resolution` (this feature provides the lookup queries the resolver runs).
- Defining `InteractionRecord`, browser-event payload structs, or artifact manifest shapes that belong to sibling epics. This feature exposes the persistence surface; sibling epics supply the types.

## Notes for the design pass

- Settle the migration story before any table lands. Forward-only versioned migrations keep recovery and retention reasoning simple; avoid ad-hoc `ALTER TABLE` paths.
- The frame-source read port shape is co-owned with the range-resolution feature (its primary consumer) — coordinate the read surface so the resolver does not need a second query path.
- The segment/index row-removal and usage-update helpers (used by both retention and recovery) live with this feature's adapter. Keep them primitive (`remove_segment`, `remove_frame_rows`, `update_usage`) so the two callers compose rather than duplicating index-mutation logic.
- An SQLite failure must stop persistence before accepting unsupported writes (foundation failure-isolation rule). Map SQLite errors to the existing `ErrorCode::PersistenceFailed` at the boundary; do not invent new error categories here.
- Default redaction of sensitive request/response values (cookies, auth, bodies) is a SPEC requirement on browser-event persistence. Apply it at the boundary before rows are written; do not rely on query-time filtering.
