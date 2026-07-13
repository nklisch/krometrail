---
id: epic-temporal-vision-toolkit-normalization-and-measurements-public-contract-tests
kind: story
stage: done
tags: [visual, testing]
parent: epic-temporal-vision-toolkit-normalization-and-measurements
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements-direct-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Prove the Deterministic Analysis Contract

## Checkpoint

Add `crates/temporal-vision/tests/analysis.rs` as a browser-free consumer proof. Build a tiny sequence with caller-owned typed frame/marker/gap IDs, borrowed decoded straight-alpha sRGB RGBA8 pixels, a declared non-black background, explicit crop/scale variants, region/mask restrictions, and a declared gap.

Exercise public normalization into owned linear RGB16 buffers, exact ordered normalization plus threshold provenance, arbitrary baseline comparison, and every adjacent comparison. Keep expected LUT/compositing values and metric vectors small enough to verify by hand. Repeated runs and repeated provenance serialization must produce identical values/bytes.

Use focused colocated tests only for private mechanics that the public fixture cannot isolate cleanly: stable lookup-table sentinel/checksum values, integer-square-root boundaries, checked retained-byte arithmetic, and conservative downscaled-mask membership. Prove resource rejection with deliberately tiny limits rather than attempting actual memory exhaustion. Do not add encoded images, browser fixtures, large snapshots, getter/derive tests, or duplicate frame-contract constructor coverage.

## Acceptance evidence

- A consumer normalizes borrowed RGBA8 frames and receives owned linear RGB16 data without importing Krometrail, CDP, MCP, image codecs, runtime, filesystem, plugin, GPU, or inferred-analysis types.
- Exact fixtures cover alpha/background conversion, crop-before-upscale, box downscale, transformed region/mask semantics, threshold equality/one-over, changed bounds, arbitrary pairs, elapsed time, and a gap boundary.
- Identical pixels/parameters produce identical normalized buffers, measurement vectors, and serialized provenance.
- Tiny processing limits and invalid scale/geometry fail explicitly before large allocation.
- The normal dependency tree remains limited to the crate's existing infrastructure-neutral dependencies.
- Formatting plus locked package and workspace check/test/clippy gates pass, with any concurrent unowned-file interference reported rather than edited.

## Ordering

Depends on `epic-temporal-vision-toolkit-normalization-and-measurements-direct-measurements`. This is the final checkpoint and validates normalization plus measurement as one downstream public seam.

## Implementation notes

- Execution capability: raised/high, selected by the autopilot caller because this public seam is the regression boundary for all downstream artifact algorithms.
- Review weight: standard (caller); child stories close on verification and the parent feature remains the review boundary.
- Files changed: `crates/temporal-vision/tests/analysis.rs`.
- Tests added: borrowed RGBA8 ownership, exact alpha/crop/upscale provenance, box downscale, transformed region/mask policy, hand-computed metrics, threshold equality/one-over, arbitrary/adjacent gap comparisons, invalid indices, and tiny processing limits.
- Verification: all 22 package tests and package clippy pass; normal dependencies remain only Serde and thiserror plus their derive machinery.
- Simplification: exact tiny values replace image fixtures and large snapshots; no getter/derive matrix or duplicate constructor coverage was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.
