---
id: epic-durable-browser-memory-sqlite-index-timeline-catalog
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-sqlite-index
depends_on: [epic-durable-browser-memory-sqlite-index-schema-migrations]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Timeline, Catalog, and Structured Gap Adapter

## Checkpoint

Implement `RecordingCatalog`, `TimelineStore`, and `CaptureGapStore` in `crates/krometrail-store/src/index/{catalog,timeline,gaps}.rs`. First observations create identity-only session/target placeholders; later catalog upserts store the generated core JSON contract without fabricating unavailable metadata.

Generic append accepts every validated observation except `Frame` and `CaptureGap`, whose authoritative paths are indexed recording and structured gap persistence. Timeline range uses inclusive bounds and the exact `(session time, frame-presence, capture ordinal, observed time, stable kind, payload sort key, insertion id)` order from the parent. Gap append inserts its structured row plus generic timeline row in one immediate transaction; gap range uses interval overlap and returns `(start,end,id)` order.

## Ordering

Depends on migrated schema and boundary codecs. It proves generic metadata behavior before frame-file composition is introduced.

## Acceptance evidence

- Placeholder rows satisfy foreign keys while `record_json IS NULL`; session/target upsert and reopen round-trip current core records exactly.
- Every observation kind round-trips via registry-derived names; unknown database names fail explicitly.
- Generic frame/gap appends reject before writing, preventing detached metadata claims.
- Equal-time fixtures return deterministic ordering; tied frame observations respect capture ordinal.
- Gap structured/timeline rows commit or roll back together, preserve all fields, and overlap a query even when the gap begins before it.
- No event content, headers, cookies, auth values, or bodies are persisted through the generic path.
- Failures remain source-safe and locked workspace gates pass.

## Implementation notes

- Implemented the three focused adapters on the shared SQLite index: catalog upserts, generic timeline metadata, and structured capture-gap persistence.
- First metadata writes create identity-only placeholders; later session/target upserts store and round-trip only validated core JSON contracts.
- Generic timeline append rejects frame and gap observations so authoritative frame/gap rows cannot detach. Inclusive range ordering follows session time, frame-presence/ordinal, observed time, registry name, payload key, and insertion id.
- Capture gaps and their generic timeline anchors share one immediate transaction; overlap reads preserve declaration time and every structured field.
- Database decode and query failures expose only stable source-safe persistence errors.
- Verification: 27 store tests passed across schema, catalog, timeline, gaps, and segment suites; store Clippy passed with warnings denied.
