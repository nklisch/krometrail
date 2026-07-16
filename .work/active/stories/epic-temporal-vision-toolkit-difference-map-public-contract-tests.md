---
id: epic-temporal-vision-toolkit-difference-map-public-contract-tests
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-difference-map
depends_on: [epic-temporal-vision-toolkit-difference-map-panel-rendering]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Public Contract and Deterministic Regression Tests

## Checkpoint

Protect the public seam and the load-bearing accumulation mechanics with a focused test suite: one browser-free integration render proving manifest correctness, PNG encoding, SHA-256 hashing, determinism, JSON round-trip, and crate independence together, plus colocated unit regressions for change-model equivalence, gap-rule equivalence, a hand-computed accumulation fixture, frequency-mode brightness ordering, and byte-bound rejection.

## Files

- `crates/temporal-vision/tests/difference_map.rs` (new)

## Integration test

Build one small browser-free synthetic sequence: typed local IDs, a declared non-black background, a region/mask restriction, a declared gap, and a few frames with known per-pixel changes. Exercise `render_difference_map` end-to-end and assert:

- the returned `DifferenceMapArtifact` exposes a manifest with `artifact_kind == DifferenceMap`, `evidence_class == SourceDerived`, the `temporal-difference-map`/`v1` algorithm, the reference frame as the only selected frame, correct counts, and the gap carried through;
- `rendered().encoding() == Png`, `rendered().bytes()` is non-empty and starts with the PNG signature, and `rendered().output_hash()` equals SHA-256 of `rendered().bytes()`;
- a second call with identical inputs produces byte-identical `bytes()` and an identical manifest (determinism);
- the manifest round-trips through JSON.

## Colocated unit regressions

- `classify_pixel_change` agrees with the existing measurement kernel at below-floor, at-floor, one-over, and far-over deltas.
- `intersecting_gap_count` matches `measure_pair`'s `GapBoundary` decision on the same sequences.
- A hand-computed 3-frame × 2×2 fixture yields exact per-pixel `change_count`, `magnitude_sum`, weighted-average timing offset, and repeated-change flag.
- `FrequencyMode::{Count, Magnitude, NormalizedFrequency}` produce the expected brightness ordering on a controlled fixture.
- Accumulator and canvas byte bounds reject oversized inputs before allocation.

## Out of scope

No full-image snapshot tests, no trivial accessor tests, no font glyph enumeration, no constructor coverage already in `contracts.rs`/`analysis.rs`.

## Acceptance evidence

- A browser-free consumer renders a complete difference map and reads its manifest without importing Krometrail, browser, codec, runtime, filesystem, or image-decoder types.
- Determinism holds across repeated renders; the PNG hash is reproducible and matches an independent SHA-256.
- The hand-computed accumulation fixture pins exact counts, magnitudes, timing offsets, and the repeated-change rule.
- Oversized accumulator/canvas inputs fail with `ResourceLimitExceeded` before allocation.
- `cargo fmt -p temporal-vision -- --check`, locked package check/test/clippy, and locked workspace check/test/clippy pass subject only to concurrently owned files documented by the orchestrator.

## Ordering constraints

Depends on `panel-rendering`. This is the final checkpoint before feature review.

## Implementation notes

- Execution capability: raised/high; the tests pin deterministic bytes, provenance, and load-bearing integer semantics without browser/runtime dependencies.
- Review weight: standard (autopilot caller).
- Files changed: `crates/temporal-vision/tests/difference_map.rs`, with focused colocated regressions in `src/difference_map.rs` and `src/measure.rs`.
- Tests added/removed: browser-free render/manifest/hash/JSON/panel/gap/output-bound contract, canonical classifier/gap equivalence, all frequency modes, and exact accumulation regressions; no full-image snapshots or accessor-only tests.
- Simplification: decoded only a few semantic pixels and the gap band instead of maintaining a brittle whole-image golden.
- Discrepancies from design: public assertions follow the shared `GeneratedArtifact::image()` seam and manifest-owned SHA-256 hash.
- Adjacent issues parked: none.
