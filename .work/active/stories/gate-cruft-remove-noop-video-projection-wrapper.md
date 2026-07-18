---
id: gate-cruft-remove-noop-video-projection-wrapper
kind: story
stage: implementing
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Remove no-op temporal-video projection wrapper

## Confidence
Medium

## Category
passthrough wrapper

## Location
`crates/krometrail-mcp/src/response.rs:619`

## Evidence

The base temporal-video mapper emits video/manifest resources but no inline images, so clearing both image collections for `inline_images: omit` is always a no-op.

## Removal

Route the registry directly through `map_temporal_video_result` and remove the projected-video wrapper and unused preference argument.

## Acceptance criteria

- The temporal-video registry route calls `map_temporal_video_result` directly.
- The no-op projected-video wrapper and its import are removed.
- Response projection input remains accepted and validated consistently, but no unused preference value is threaded into video mapping.
- Existing temporal-video response/resource tests remain green.

## Implementation plan

- Keep projection argument splitting at the MCP boundary for schema/validation compatibility.
- Discard the validated preference for the video route and invoke the canonical mapper directly.
- Remove the wrapper and any now-unused imports.
