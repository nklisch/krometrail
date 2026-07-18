---
id: gate-cruft-remove-noop-video-projection-wrapper
kind: story
stage: drafting
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
