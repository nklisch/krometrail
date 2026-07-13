---
id: epic-temporal-vision-toolkit-storyboard-public-contract-tests
kind: story
stage: implementing
tags: [visual, testing]
parent: epic-temporal-vision-toolkit-storyboard
depends_on: [epic-temporal-vision-toolkit-storyboard-rendering]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Prove the Storyboard Artifact Contract

## Checkpoint

Add `crates/temporal-vision/tests/storyboard.rs` as a browser-free public consumer proof. Build a tiny typed-ID sequence with distinct required anchors, equal timestamps, stable and changing intervals, local peaks, a changed-region appearance/disappearance, multiple markers, and a declared gap separating continuity segments. Normalize it with the existing public API, then generate a three-tile and default-eight storyboard plus orientation.

Assert exact selected source IDs, reasons, role mappings, over-budget omissions, source order, marker buckets, segment-first gap behavior, and orientation fallbacks. Prove PNG signature/dimensions, selected tile colors, exact manifest source/provenance/hash agreement, manifest round trip, and repeated byte determinism. Use one small committed hash for a fixed tiny raster rather than a large binary golden artifact.

Keep private tests focused on score tuple/tie order, inverse sRGB endpoint/tie behavior, font escaping/ellipsizing, marker assignment, gap intersection, checked layout arithmetic, and bounded PNG writing. Verify visible semantic text/pattern regions through deterministic glyph/layout evidence rather than OCR. Exercise tiny limits instead of allocating near production maxima.

## Acceptance evidence

- An infrastructure-free consumer with arbitrary typed IDs creates both artifact kinds and traces every rendered panel to retained source IDs.
- Fixtures protect hard 3–12 limits, required-anchor conflict disposition, exact timestamp/index ties, every change-aware score component, and no cross-gap calculation.
- Repeated output has identical selected IDs, PNG bytes, SHA-256, parameters, and manifest; decoded dimensions and tile pixels are correct.
- Visible `GAP`, `TIME →`, before/during/after, timestamp/offset, frame, reason, and marker labels are verified without diagnosis claims or fragile full-image snapshots.
- Tiny layout/canvas/encoded limits fail explicitly and do not return partial artifacts.
- Normal dependencies remain Krometrail/CDP/MCP/runtime/UI/font/filesystem/GPU-free and add only bounded PNG encoding plus SHA-256.
- Locked formatting, package/workspace check, test, and clippy gates pass, with concurrent unowned-file interference reported rather than edited.

## Ordering

Depends on `epic-temporal-vision-toolkit-storyboard-rendering`. This final checkpoint verifies selection, rasterization, encoding, and provenance as one public artifact seam.
