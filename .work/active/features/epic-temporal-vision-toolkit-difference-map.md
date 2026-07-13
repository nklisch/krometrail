---
id: epic-temporal-vision-toolkit-difference-map
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

# Temporal Difference Map

## Brief

This feature renders a temporal difference map that shows where pixels changed during an interval and when those changes occurred.

The artifact contains three coordinated panels: a reference source frame for spatial context, a change-frequency panel showing how often each pixel or region changed, and a change-timing panel showing when observed changes occurred. The frequency panel records whether brightness represents count, magnitude, or normalized frequency. The timing panel uses a declared time palette with numeric start, midpoint, and end labels. Pixels that change repeatedly across widely separated moments receive a repeated-change indicator rather than a falsely precise single timestamp.

Thresholded change detection and gap handling are reused from the normalization-and-measurements feature. The output includes legends for frequency, timing, and repeated-change indicators, and a visible warning when the source interval contains declared capture gaps.

This feature does not diagnose why a region changed, track logical elements, or infer motion direction. It exposes spatial and temporal change patterns as a source-derived artifact.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — parallel to storyboard after measurements land

## Simplification opportunity

- Render the three panels into one combined image with a simple fixed layout rather than building a composable panel engine.
- Use the same thresholded pixel-difference metric as storyboard selection rather than introducing a separate change model.
- Keep the time palette small and deterministic; additional palettes can be added later without changing the core contract.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Temporal Difference Map, Visual-Change Measurements, Capture Gaps
- `docs/EVALUATION.md` — Difference-map evaluation criteria

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
