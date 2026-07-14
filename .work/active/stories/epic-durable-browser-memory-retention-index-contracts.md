---
id: epic-durable-browser-memory-retention-index-contracts
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: [epic-durable-browser-memory-retention-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# SQLite Retention Sequence, Usage, Pin, and Deletion Contracts

## Checkpoint

Extend the existing SQLite migration registry to v2 and add store-private retention/deletion queries. Backfill and allocate stable global segment retention sequence, keep the existing classed usage table authoritative, implement exact-range pins and distinct pinned usage, select provenance-dependent artifacts, and persist replayable deletion batches/items. Do not add another schema runner, range resolver, scanner, or manifest parser.

## Ordering

Depends on core contracts so query results and status snapshots use settled domain values.

## Acceptance evidence

- V1 migrates transactionally and idempotently to contiguous v2; failed/future migrations retain existing refusal/rollback behavior.
- Interleaved sessions receive one durable sequence per segment; candidate order is sequence then segment id and never compares session-relative time globally.
- Usage covers exact segment/index/browser-event/artifact rows with checked totals, distinct pin bytes, open bytes, retained endpoints, pending bytes, and bounded SQLite accounting slack.
- Exact and overlapping pin operations are idempotent and never make a protected segment eligible.
- Artifact candidates include every artifact referencing any frame selected for deletion.
- Deletion journal rows round-trip every object key/path/usage fact required for deterministic replay after reopen.

## Implementation notes

- Added contiguous schema v2 with durable immutable segment retention sequences, deterministic v1 row-order backfill, deletion journal tables, replay-state index, and insert/update guards.
- Segment registration now allocates one sequence once and updates the authoritative segment usage row in the same transaction as searchable metadata.
- Added exact range pins, overlap-safe distinct pinned accounting, global sequence candidate selection, classed checked usage snapshots, provenance-dependent artifact selection, and typed deletion journal prepare/replay/metadata/finalize operations.
- Updated the schema inventory/future-version qualification and added deterministic sequence/pin/usage coverage. No second migration runner, scanner, resolver, or manifest parser was introduced.
- Verification: `cargo test -p krometrail-store --locked` passed (51 tests); formatting and diff checks passed. Dead-code warnings are expected at this checkpoint and are consumed by the immediately dependent removal engine.
