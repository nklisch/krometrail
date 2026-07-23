---
id: feature-perf-artifact-pixel-pipeline-opt-2
kind: story
stage: implementing
tags: [perf]
parent: feature-perf-artifact-pixel-pipeline
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Filmstrip subsequence normalization + cropped-opaque fast path

Optimization 2 of the parent feature. Independent of the classification path;
fixes an outright failure mode (120-frame default-limits filmstrip). ~4–5×. See
the parent feature body for Units 2.1–2.2 and the correctness proof.

## Scope

- `generate_region_filmstrip` / `render_filmstrip` (`filmstrip.rs:906-1010,
  1231-1259`): normalize only the selected tile subsequence (`plan.tiles()`
  frames, distinct + strictly increasing) + the already-separate locator, instead
  of the whole cropped 120-frame source. Render indexes the subsequence by tile
  **position** (`enumerate` index at `1248`), not `tile.frame_index()` (`1254`).
  Selection (`select_indices`) is untouched, so tile semantics are identical.
- Extend the opaque fast path to cropped opaque frames
  (`normalize.rs:290, 521-573`): drop the `crop == full frame` requirement in
  `can_use_opaque_full_frame_fast_path`; add `normalize_opaque_crop` walking only
  crop rows (identity → `linear_channel`; downscale → box-average with crop
  origin). Byte-identical to `normalize_frame_general` for opaque input because
  `composited_pixel(alpha=255) = linear_channel`.

## Determinism / regression

- Filmstrip `output_hash`, manifest, and image bytes byte-identical to the
  current full-normalization output (equivalence test).
- **Regression:** a 120-frame default-limits filmstrip request succeeds (pins the
  former `normalized retained bytes 345600000 exceeds limit 67108864` failure).
- Extend `opaque_full_frame_fast_path_matches_reference_across_rectangular_scales`
  with cropped opaque cases; non-opaque / masked input still uses the general path.

## Acceptance

- [ ] Parent Unit 2.1–2.2 acceptance criteria met.
- [ ] New `region_filmstrip_perf.rs` bench shows ~389 ms → ~80 ms; 120-frame
      success regression green.
- [ ] `cargo test -p temporal-vision` green; clippy clean.

## Implementation notes

The filmstrip now normalizes only the selected tile subsequence and its separate
locator, then remaps rendering by subsequence position. Cropped opaque identity
and box-downscale fast paths match the general reference. The release benchmark
measured 135,429 µs for the 120-frame 1224×958 case, retaining 5,111,808 bytes
for 12 tiles plus the locator; the bounded 120-frame success regression passes.
