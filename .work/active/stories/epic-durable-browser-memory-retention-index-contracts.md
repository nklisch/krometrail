---
id: epic-durable-browser-memory-retention-index-contracts
kind: story
stage: implementing
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
