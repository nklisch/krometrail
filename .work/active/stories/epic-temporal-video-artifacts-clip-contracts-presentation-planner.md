---
id: epic-temporal-video-artifacts-clip-contracts-presentation-planner
kind: story
stage: done
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

## Implementation notes

- Execution capability: GPT-5.6 Sol at xhigh, selected by the active autopilot caller for the stable temporal/provenance logic and exact-limit behavior.
- Review weight: `standard` (autopilot default); this child checkpoint closes on green verification and the integrated feature receives independent review.
- Files changed: `src/video/{mod.rs,plan.rs,tests.rs}` and the root module declaration in `src/main.rs`.
- Tests added: ordinary and tied frame deltas, single-frame terminal hold, meaningful-frame and gap-slate model holds, deterministic gap-order serialization, clip/coalesce behavior, frame/gap boundary splitting, fully obscured meaningful-frame rejection, exact/next segment limits, and presentation-duration overflow without truncation.
- Simplification: one immutable draft-segment path drives both policies; model optimization adjusts only explicit durations and timing bases before rebuilding contiguous offsets. No I/O, image processing, FFmpeg, provider, or mutable global state was introduced.
- Discrepancies from design: declared gaps can cover retained frame timestamps; an explicitly meaningful frame that receives no visible segment after gap replacement now fails `invalid_input` instead of silently claiming a model hold. All input frame IDs remain ordered plan provenance even when a gap slate replaces their presentation interval.
- Verification: `cargo fmt --all -- --check`, `cargo clippy --bin krometrail --all-targets -- -D warnings`, focused video tests (7 passed), and the complete root binary test suite (107 passed, 2 ignored manual qualifications) passed.
- Adjacent issues parked: none.
