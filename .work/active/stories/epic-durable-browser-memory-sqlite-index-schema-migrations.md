---
id: epic-durable-browser-memory-sqlite-index-schema-migrations
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-sqlite-index
depends_on: [epic-durable-browser-memory-sqlite-index-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Versioned SQLite Schema and Startup Migrations

## Checkpoint

Add exact `rusqlite = 0.33.0` with `default-features = false` and `bundled`, then implement `crates/krometrail-store/src/index/{mod,codec,migrations,schema_v1}.rs`. Schema v1 is the exact strict-table/index contract in the parent feature: sessions, targets, segments, frames, generic timeline observations, capture gaps, pins, pin-segment links, artifacts, artifact-frame provenance, and usage.

`SqliteIndex::open(IndexStoreConfig { database_path, segments_directory, busy_timeout })` creates the parent directory, opens one synchronous connection, bounds lock contention, verifies foreign keys/WAL/FULL synchronous mode, and applies contiguous forward-only migrations under an exclusive transaction. IDs use 16-byte BLOBs; full-domain `u64` values use order-preserving big-endian BLOBs; `i128` source time uses a lossless 16-byte BLOB.

## Ordering

Depends on core contracts so schema codecs and migration tests use final stable names and domain values.

## Acceptance evidence

- A Rust-1.85-constrained dependency resolution locks rusqlite 0.33.0 and bundled SQLite; no database type or dependency enters core.
- Fresh v0→v1 migration commits atomically; v1 reopen is a no-op; forced failure rolls back; a future `user_version` prevents startup without mutation.
- File-backed tests verify WAL, foreign keys, FULL synchronous mode, and bounded busy timeout.
- ID/u64/i128 codecs round-trip boundary values; SQL BLOB sort order equals Rust unsigned order including values above `i64::MAX`.
- Schema inventory matches the parent SQL exactly and has no raw browser-event payload/body/header column.
- Adapter failures map to source-safe `PersistenceFailed`; locked workspace gates pass.

## Implementation notes

- Locked exact `rusqlite` 0.33.0 with bundled SQLite and disabled defaults; core remains database-free.
- Added one file-backed connection boundary with foreign keys, WAL, FULL synchronization, caller-bounded busy timeout, and contiguous forward-only migration handling.
- Schema v1 implements the complete strict-table and index inventory from the feature design without a raw browser-event payload path.
- UUID, full-domain unsigned integer, and signed source-time codecs are fixed-width and boundary-tested; unsigned BLOB sorting matches Rust ordering.
- Qualification covers settings, fresh migration, idempotent reopen, forced rollback, future-version refusal, inventory, redaction-by-omission, and source-safe startup errors.
- Verification: 23 store tests passed; store Clippy passed with warnings denied; dependency tree resolves `rusqlite v0.33.0` exactly.
