---
id: epic-durable-browser-memory-range-resolution-store-queries
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-range-resolution
depends_on: [epic-durable-browser-memory-range-resolution-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Add focused resolver query ports and SQLite support

## Checkpoint

Extend the existing focused storage ports rather than creating a god-port. `SqliteIndex` should answer the resolver's required metadata questions using the current schema and the same frame address/read helpers used by source-frame retrieval.

## Required contract

- `RecordingCatalog` gains `session` and `target` reads; NULL placeholder rows are not treated as complete records.
- `FrameSource` gains `frame_metadata_by_id` and `frames_in_ordinal_range`; both reuse the established frame address/CRC/context path.
- `TimelineAnchorSource` looks up typed marker/navigation timeline observations by payload and can ask for the latest observation of a kind in one session/target.
- `InteractionAnchorSource` returns existing `InteractionAnchor` projections when available; until sibling browser features persist anchors, `SqliteIndex` returns `Ok(None)` instead of fabricating interaction rows.

## Acceptance evidence

- [x] Source-frame ranges are read through `FrameSource`, not a second SQL frame path.
- [x] Frame ordering is deterministic by per-target `CaptureOrdinal`, including tied timestamps and multiple targets.
- [x] Marker/navigation lookup works from generic `timeline_observations` without structured marker/navigation tables.
- [x] Wrong payload kind, absent anchor, and placeholder catalog rows produce source-safe outcomes.
- [x] Core ports still do not expose `rusqlite`, paths, CDP, MCP, Tokio, or temporal-vision types.

## Implementation

Extended `RecordingCatalog` and `FrameSource` with domain-only metadata/ordinal reads. `SqliteIndex` now decodes complete catalog records, frame metadata, and ordinal-ordered frame payloads through the existing address/CRC reader. Added typed marker/navigation lookup over generic timeline rows and an explicit empty interaction-anchor adapter until browser-operation persistence lands. Store tests and locked checks pass for the focused adapter surface.
