---
id: gate-cruft-remove-noop-video-projection-wrapper
kind: story
stage: done
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

## Implementation notes

- The temporal-video route still splits and validates response projection input at the MCP boundary.
- The validated but irrelevant preference is discarded, and the route now calls the canonical mapper directly.
- Removed the no-op wrapper that could only clear already-empty inline-image collections.

## Validation

- `cargo test -p krometrail-mcp --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Review

- Verdict: pass; validation remains at the protocol boundary and the canonical mapper preserves the video/resource contract.
- Effective implementation size: small. Effective review weight: standard bounded inline standalone-story review.
- No review findings remained after verification.
