---
id: epic-durable-browser-memory-range-resolution-resolver-semantics
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-range-resolution
depends_on: [epic-durable-browser-memory-range-resolution-store-queries]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Implement deterministic range resolver semantics

## Checkpoint

Implement `TemporalRangeResolver` in core over injected `RecordingCatalog`, `FrameSource`, `CaptureGapStore`, `TimelineStore + TimelineAnchorSource`, and `InteractionAnchorSource` ports. Every temporal consumer receives a `ResolvedRange`; no consumer reinterprets natural anchors.

## Required behavior

- Explicit session-time ranges require one session and one target.
- Wall-clock ranges convert checked offsets from `RecordingSession::started_at`; no Chrome `SourceTime` is treated as a wall clock.
- Source-frame ranges validate endpoint frames share one session/target, match caller scope, and order by inclusive capture ordinal.
- Marker/navigation anchors resolve from typed generic timeline observations with optional windows.
- Interaction/latest-interaction anchors resolve from durable `InteractionAnchor` records when present and otherwise fail honestly with `NotFound`.
- Default implicit interaction policy is `started_at - 150ms` saturating at zero through `observed_at.unwrap_or(completed_at) + 250ms` checked for overflow.
- Finalization reads frames through `FrameSource`, gaps through `CaptureGapStore`, timeline IDs through `TimelineStore::range`, and returns exact requested/resolved bounds.

## Acceptance evidence

- [x] Session-time and equivalent wall-clock ranges return identical frame IDs.
- [x] Source-frame ranges include both endpoints and all retained ordinal-interior frames, even with timestamp ties.
- [x] Gap include/reject and retention allow/strict policies are covered.
- [x] Wrong-target/wrong-session anchors return `InvalidInput`; absent anchors and no retained evidence return `NotFound`; overflow returns `InvalidTime`.
- [x] Resolved ranges include related marker/navigation/interaction IDs from timeline order while frame IDs remain from the frame source path.

## Implementation

Implemented `TemporalRangeResolver` over the focused catalog, frame, gap, timeline-anchor, timeline, and interaction-anchor ports. Explicit session/wall-clock, source-frame, marker/navigation, and interaction policies now converge on one inclusive `ResolvedRange`; retention and gap behavior produce source-safe errors and warnings. Interaction anchors remain `NotFound` while durable browser-operation persistence is absent. Qualification coverage is in `crates/krometrail-store/tests/range_resolution.rs`.
