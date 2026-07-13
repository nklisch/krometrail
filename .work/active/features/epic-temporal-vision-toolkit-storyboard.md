---
id: epic-temporal-vision-toolkit-storyboard
kind: feature
stage: drafting
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Temporal Storyboard and Orientation Composite

## Brief

This feature delivers deterministic representative-frame selection and the primary temporal storyboard artifact, plus the simple before/during/after orientation composite.

The storyboard preserves required anchors when available: the last source frame before the range anchor, the first frame after the anchor, the first measurable visual change, the frame with the greatest difference from the pre-action baseline, and the final retained frame. Remaining tiles are selected from captured source frames using deterministic visual-change measurements, favoring local change peaks, trend changes, appearance/disappearance of changed regions, information gain relative to selected neighbors, and temporal coverage of long intervals.

Each tile displays its session-relative timestamp, offset from the query anchor, source-frame identifier, intersecting marker labels, and gap indication. The layout preserves source aspect ratio, avoids decorative borders that can be mistaken for page content, and supports a caller-configurable tile count within the 3–12 range. The default bundle uses no more than eight tiles.

The before/during/after composite selects three frames by the same deterministic rules and lays them out as a low-complexity orientation entry point. "During" is selected by a declared rule and identifies its source frame; it is not generated or averaged.

This feature does not render difference maps, region filmstrips, or motion-history images. It produces a single storyboard image and optionally the orientation composite.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: primary artifact feature — consumes normalized pixels and measurements

## Simplification opportunity

- Implement one deterministic selection algorithm and version it in provenance rather than exposing multiple strategies.
- Render labels and time-direction indicators with simple, high-contrast drawing rather than pulling in a full UI layout engine.
- Fold before/during/after into the same feature because it reuses the same frame-selection logic and rendering primitives.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Temporal Storyboard, Before/During/After Composite, Shared Artifact Contract, Determinism
- `docs/EVALUATION.md` — Storyboard evaluation criteria

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
