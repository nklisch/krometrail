---
id: story-temporal-resolve-range
kind: story
stage: done
tags: [browser]
parent: feature-temporal-range-artifact-economy
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# resolve_temporal_range tool

Unit 1 of the parent design: new registry-declared tool taking the bundle's natural-anchor query, returning resolved range summary, minted range_handle, and capture quality - no artifact generation, no browser events. Handle round-trips into every handle-capable tool. SPEC Temporal Ranges/Queries roll forward.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.

## Completion Notes

Implemented the resolver operation and capture-quality-only service seam,
registered it with route/schema validation, and wired range-handle output into
the existing handle-capable request path.
