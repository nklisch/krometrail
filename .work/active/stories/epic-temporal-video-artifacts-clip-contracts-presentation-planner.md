---
id: epic-temporal-video-artifacts-clip-contracts-presentation-planner
kind: story
stage: implementing
tags: [visual, security]
parent: epic-temporal-video-artifacts-clip-contracts
depends_on: [epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Deterministic temporal-video presentation planner

## Design checkpoint

Implement the pure one-epoch planner that converts validated retained-frame metadata, declared meaningful frame IDs, and explicit capture gaps into contiguous real-time or model-optimized presentation segments. It owns deterministic gap clipping/coalescing and typed duration adjustments, but no image decoding, gap-slate pixels, FFmpeg invocation, persistence, MCP routing, or importance inference.

## Acceptance evidence

- Table tests prove identical serialized plans for identical semantic input, including when gaps arrive in another order.
- Boundary cases cover tied timestamps, one frame, terminal holds, meaningful holds, gap overlap/clipping/coalescing, frames within gap ranges, no interpolation, one-epoch enforcement, and every exact/next-unit ceiling.
- Every plan begins at presentation zero, is contiguous/non-empty, maps every segment to retained frame or gap provenance, and fails rather than truncating when limits are exceeded.

## Ordering constraints

- Depends on `epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port`.
- The provenance checkpoint embeds the planner's exact output and may not recalculate timing independently.
