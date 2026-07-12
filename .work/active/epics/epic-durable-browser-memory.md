---
id: epic-durable-browser-memory
kind: epic
stage: drafting
tags: [storage, browser]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
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

## Anticipated child features

- Versioned append-only frame segment format and writer
- SQLite metadata schema and timeline index
- Interaction, marker, capture-gap, and browser-event persistence
- Natural-anchor and explicit temporal range resolution
- Disk-budget accounting, pinning, and oldest-first eviction
- Segment recovery, index reconciliation, and session deletion

<!-- The design pass on each child feature will fill in real specifics. -->
