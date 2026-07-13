---
id: epic-durable-browser-memory-recovery
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

# Crash Recovery, Open-Segment Sealing, and Index Reconciliation

## Brief

Own the startup-time consistency capability of the recording store: after a crash or unclean shutdown, locate unsealed segments, scan complete frame records, truncate incomplete trailing data, seal recoverable segments, and reconcile the SQLite frame index and usage accounting against what the segment files actually contain. The invariant this feature enforces is the SPEC's "metadata does not claim that a frame exists until its complete segment record is durable" — after recovery, every frame the index claims exists is backed by a complete, durable segment record.

Recovery runs once at store open, before retention or capture begin. It treats the SQLite index as reconcilable metadata and the segment files as the byte-level authority for what was actually persisted. Pins live in SQLite (WAL-durable) and are trusted across recovery; recovery does not reconstruct them. It reports the open-segment-beyond-budget tolerance required by the evaluation.

This feature does not own the segment byte format, the SQLite schema, runtime eviction, or range resolution. It is the startup consistency pass that makes the store safe to use after a crash.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the segment-format and SQLite-index features; runs at store open, before retention. Independent of the retention and range-resolution features at the dependency-graph level.
- Design decisions inherited: recovery-before-retention startup ordering; the recoverable-record layout (length-prefix plus checksum) is owned by the segment-format feature and consumed here; pins are trusted across recovery because they are SQLite metadata, not frame payloads; the index is the reconcilable metadata authority and the segment files are the byte-level authority.

## Simplification opportunity

- Reuse the same primitive helpers (`remove_segment`, `remove_frame_rows`, `update_usage`) supplied by the SQLite-index feature for orphan removal. Recovery's reconciliation is "remove index rows whose backing segment record is incomplete or absent" — the same removal primitives retention uses, with a different predicate.
- Trust the segment file's per-record checksum and length-prefix as the recovery authority. Do not maintain a parallel recovery journal; the sealed-footer + record-checksum format is already a recoverable record format by design.
- The open-segment-beyond-budget tolerance is a reported measurement, not a hard failure: at most one open segment may exceed budget while recording, and recovery reports the bound rather than refusing to open.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Disk Budget and Retention (stopping a session flushes accepted frames and metadata before reporting completion), Errors and Degraded Operation (an unrecoverable browser connection ends the session after flushing accepted data)
- `docs/ARCHITECTURE.md` — Recording Store (Crash Recovery), Frame Ingestion (ack-then-handoff ordering), Failure Isolation (process shutdown waits for bounded flushing and then reports incomplete work)
- `docs/EVALUATION.md` — Storage and Retention Evaluation (crash recovery restores complete records and removes incomplete trailing writes; deletion removes all data belonging to the selected session)

## Scope and honest non-goals

**In scope:**

- Store-open recovery routine: locate every unsealed segment in the data directory, scan its complete frame records, truncate incomplete trailing bytes, seal the segment with a sealed footer.
- Frame-index reconciliation: insert index rows for durable frames missing from the index; remove index rows whose backing segment record is incomplete or absent; preserve the `(segment_id, byte_offset)` addressing contract from the segment-format feature.
- Usage-accounting reconciliation: recompute usage from the reconciled segments and index state so retention's status surface and eviction decisions start from a correct number.
- Pin preservation: pins in SQLite are trusted across recovery; no pin reconstruction pass.
- The open-segment-beyond-budget tolerance measurement and its status-surface report.
- The write-order guarantee and its test: a frame's segment record is durable before its index row is committed, so a crash between the two always leaves an orphan index row (recovered by removal) rather than a dangling payload.

**Non-goals:**

- The segment byte format and writer — owned by `epic-durable-browser-memory-segment-format`. Recovery consumes the format; it does not define it.
- The SQLite schema, migrations, and removal helpers — owned by `epic-durable-browser-memory-sqlite-index`. Recovery calls those helpers.
- Runtime eviction, paused-budget state, and session deletion — owned by `epic-durable-browser-memory-retention`. Recovery produces a consistent starting state; retention operates on it afterward.
- Range resolution — owned by `epic-durable-browser-memory-range-resolution`.
- Reconstructing pins, interaction records, or browser events not already in SQLite. WAL-durable metadata is the authority; recovery does not second-guess it.

## Notes for the design pass

- The recoverable-record layout (length-prefix + checksum per record, sealed footer) is the contract this feature depends on. Verify in this feature's design pass that the segment-format feature's record boundary lets recovery identify the last complete record without parsing payload contents.
- The write-order test (segment-record durable before index commit) is co-owned with the segment-format feature. The two features must agree on whose test suite owns the cross-cutting assertion; recommend a single integration test in this feature that exercises a real crash mid-write.
- Recovery must be idempotent: running it twice on an already-recovered store must be a no-op (all segments already sealed, index already reconciled). This is the only safe way to handle a crash during recovery itself.
- Map recovery failures to the existing `ErrorCode::PersistenceFailed` (or `ShutdownIncomplete` for a recovery that gives up on a corrupted store) at the boundary; do not invent new error categories here.
