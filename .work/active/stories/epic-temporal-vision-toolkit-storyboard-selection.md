---
id: epic-temporal-vision-toolkit-storyboard-selection
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-storyboard
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Select Deterministic Representative Frames

## Checkpoint

Implement `crates/temporal-vision/src/select.rs` and expose `StoryboardTileLimit`, `SelectionReason`, `SelectedFrame`, `OmittedAnchor`, `StoryboardSelection`, and `select_storyboard_frames` through `src/lib.rs`.

Validate that the source `FrameSequence` and `NormalizedSequence` have identical frame counts, IDs, timestamps, and normalized geometry, and that the anchor lies in the source range. Accept tile limits 3–12 with default 8. Resolve and merge core anchors, then admit distinct frames in priority order: pre-anchor baseline, peak baseline change, final frame, first measurable change, first post-anchor frame. Record available anchors displaced by the hard limit rather than exceeding it. Add marker and retained gap-side frames as supplementary boundary candidates.

Use the existing adjacent and pair measurement kernels. Declared gaps partition continuity segments and prohibit every cross-gap baseline, local-peak, trend, and information-gain calculation. Fill remaining tiles by the exact version-1 lexicographic score: unrepresented segment, supplementary boundary count, cumulative adjacent-change information gain, local change peak, trend delta, changed-region appearance/disappearance, temporal coverage, then earlier source declaration index. Use checked integer arithmetic and return the final selection as an ordered unique source subsequence.

Resolve orientation from the same plan: before is the strict pre-anchor baseline (or first frame), during is peak baseline change (or first strict post-anchor frame, then baseline), and after is the final retained frame. Role mappings may repeat on short/unchanged sequences, while selected IDs remain unique.

## Acceptance evidence

- Limits 3/12 and default 8 behave exactly; 2/13 fail and no result exceeds its hard limit.
- Distinct and merged core anchors, over-budget omissions, marker boundaries, gap-side boundaries, and tied timestamps follow the declared priorities and source-index tie break.
- First change and peak baseline use direct descriptive measurements; no comparison or cumulative score crosses a declared gap.
- Remaining selection demonstrably considers segment coverage, local peaks, trend deltas, region transitions, information gain, and temporal coverage.
- Orientation fallbacks use exact source frames and make no interpolation, averaging, inferred motion, reversal, defect, or causal claim.
- Repeated selection and serialization are value/byte identical for the same input.

## Ordering

First checkpoint. It consumes the implemented normalization/measurement contract and provides the sole selection plan used by both renderers.

## Implementation notes

- Added the versioned public selection plan and one stable reason registry in `crates/temporal-vision/src/select.rs`.
- Exact core-anchor priority, hard-limit omissions, marker/gap boundary admission, gap-partitioned cumulative scoring, and earlier-index tie-breaking are integer-only and deterministic.
- Source and normalized sequence identity/time/geometry alignment is validated before selection; orientation roles retain exact source indices and use the declared baseline/peak/post/final fallbacks.
- Focused selection tests cover 3-tile anchor pressure, invalid limits, stable reason serialization, and repeated output determinism.
- Verification: package formatting, locked all-target check, and focused selection tests passed.
