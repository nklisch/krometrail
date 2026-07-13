---
id: epic-temporal-vision-toolkit-normalization-and-measurements
kind: feature
stage: drafting
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-frame-sequence-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Normalization and Visual-Change Measurements

## Brief

This feature turns raw source frames into a common pixel representation and computes the direct visual-change measurements used by every artifact in the crate.

Normalization supports the source-derived transformations allowed by `docs/VISUAL-EVIDENCE.md`: color-space conversion to a common working space, alpha compositing against a declared background, integer scaling with recorded parameters, fixed cropping, and light denoising or thresholding with recorded parameters. Each normalization step records its parameters in provenance.

Measurements are descriptive, not diagnostic. The crate computes: absolute pixel difference, changed-pixel proportion, changed-region bounds, luminance difference, color difference, perceptual frame distance, and elapsed time since the preceding captured frame. Noise thresholds are configurable and appear in provenance. Default thresholds reduce encoding and anti-aliasing noise without claiming to remove all irrelevant change.

This feature does not select frames or render final artifacts. It exposes normalized pixel buffers and a measurement vector that storyboard, difference-map, region-filmstrip, and motion-history features consume.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: second foundation feature — provides the prepared pixels and metrics every artifact depends on

## Simplification opportunity

- Start with a single working pixel representation (e.g., linear RGBA8 or a small set of supported formats) rather than a pluggable color-management pipeline.
- Keep perceptual distance simple and deterministic; avoid heavyweight perceptual models until evaluation shows they improve selection or artifact interpretation.
- Record every transformation parameter in provenance rather than hiding defaults, so reproducibility is explicit.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Normalization, Visual-Change Measurements, Determinism
- `docs/ARCHITECTURE.md` — Temporal Visual Crate

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
