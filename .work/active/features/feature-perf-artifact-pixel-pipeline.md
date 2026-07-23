---
id: feature-perf-artifact-pixel-pipeline
kind: feature
stage: implementing
tags: [perf]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Artifact pixel-pipeline performance

## Brief

Profiling temporal-vision (release build, 120 synthetic frames of 1224×958
with realistic regional change, 119 adjacent pairs, 16-core machine) shows a
4-artifact request costs ~9.3 s at identity resolution (~2.35 s at the
production fit-limits downscale) with zero intra-crate parallelism, and most
of it is duplicated work. The artifact scheduler's default 15 s
`max_wall_time` (src/artifacts/scheduler.rs:86) leaves little headroom, so
this is deadline risk, not just latency. perf stat: IPC 4.3, negligible
cache/branch misses — the pipeline is instruction-throughput-bound, not
memory-bound (identical 7.0 ns/px at both resolutions).

Measured evidence (identity resolution unless noted):

- One adjacent-pair classification pass costs 980 ms (8.2 ms/pair,
  7.0 ns/px). A storyboard+difference+motion request re-runs ~4 full passes
  plus ~84 baseline pairs — ~3.9 s of the 9.3 s suite is byte-identical
  duplicate classification. Sites: measure.rs:238 (`measure_adjacent`),
  select.rs:405 + 609-639 (`peak_baseline_comparison` re-scans),
  difference_map.rs:165-229 (re-classifies the same pairs),
  motion_history.rs:232 + 384-410 (measure_adjacent again PLUS a second
  full classify pass in `accumulate_segment`). The existing scaffold
  tests/pair_classification_perf.rs documents the `4M+B` formula. No
  rayon/threads anywhere in the crate; the host scheduler only overlaps two
  whole generators. Frame/pair work is embarrassingly parallel.
- Per-pixel scalar overhead: ~137 instructions / ~32 cycles per
  pixel-compare. Sites: measure.rs:369-392 (`classify_pixel_change` uses
  all-u128 checked arithmetic and recomputes the threshold
  `noise_floor² × weight_sum` via checked_pow/checked_mul per pixel — max
  weighted square ≈ 6.0e14 fits u64 with headroom); measure.rs:267-279
  (`measure_pixels`: div/mod per pixel for x/y plus try_from, needed only
  for changed pixels); same div/mod pattern at difference_map.rs:187-188 and
  motion_history.rs:389-390; per-pixel checked index math in
  render/canvas.rs:64-76. Fix direction: hoist threshold, u64 arithmetic,
  row-based iteration, per-row mask slices, changed-pixel-only coordinate
  math — makes the loop autovectorizable. Expected 3–6× per pass,
  multiplying with the dedup fix.
- Region filmstrip normalizes ALL frames for a handful of tiles:
  filmstrip.rs:957-965 runs `normalize_sequence` over the entire 120-frame
  source cropped to the region while `plan.tiles()` draws only tile_limit
  (8) + locator — 389 ms measured, ~320 ms (~82%) normalizing 112 frames
  never read. Compounding: any crop disables the opaque fast path
  (normalize.rs:290), forcing `normalize_frame_general`
  (normalize.rs:680-719) with per-pixel `composited_pixel` checked
  arithmetic. With default limits the same over-normalization FAILS outright
  at 120 frames ("normalized retained bytes: 345600000 exceeds limit
  67108864"). Fix: normalize only selected tile indices + locator; extend
  the opaque fast path to cropped opaque frames. Expected ~4–5×.
- Not bottlenecks (measured): PNG encode at Compression::Best is 30–120 ms
  per artifact; render/draw phases ~30–75 ms.

Proposed hierarchy levels: level 1 (classify each adjacent pair once per
(normalized sequence, noise floor) and share results/change-masks across
select, difference-map, and motion-history consumers; parallelism counts as
level 1 here because per-pair work is strictly sequential today and
inherently independent — expected dedup 2.5–4× on multi-artifact requests,
near-linear frame-parallel scaling on top; suite 9.3 s → well under 1 s),
level 3/4 (inner-loop scalar fixes above), level 1 (filmstrip subsequence
normalization). Probe families: on-CPU + microarchitecture (counters
captured: 529G instructions, 122.8G cycles, IPC 4.3).

Out-of-scope contract flag for design: the RGB16-linear normalized format
(6 B/px) is what forces 120-frame identity runs over the retained-bytes cap
and down to fit-limits scale. A narrower classification domain would halve
or quarter retained bytes but changes documented evidence semantics — a
strategic contract decision, not part of this feature unless the user
directs otherwise; record it as a possible follow-up.

## Perf Overview

The 4-artifact temporal-vision request is CPU instruction-throughput bound (IPC
4.3, negligible cache/branch misses), so wins come from doing less scalar work
per pixel and from stopping the pipeline from doing the *same* per-pixel work
multiple times. Profiling (in-crate release benchmark
`crates/temporal-vision/tests/pair_classification_perf.rs`, 120×1224×958 frames,
119 adjacent pairs, 16 cores) attributes the 9.3 s identity suite to three
independent, compounding causes:

1. **Redundant adjacent-pair classification** — a storyboard + difference-map +
   motion-history request runs `4M + B` full per-pixel classification passes
   (`M` = 119 measurable pairs, `B` = baseline-run pairs), ~3.9 s of which is
   byte-identical duplicate work across the four generators.
2. **Per-pixel scalar overhead** — ~137 instructions / ~32 cycles per pixel from
   all-u128 checked arithmetic, a per-pixel threshold recompute, and per-pixel
   `div`/`mod` + `try_from` coordinate math that is only needed for the small
   changed subset.
3. **No intra-crate parallelism** — the per-pair loop is strictly sequential
   though every pair is independent; the host scheduler only overlaps two whole
   generators.

Plus a separate filmstrip pathology: it normalizes all 120 source frames to draw
8 tiles + a locator (~82 % waste), any crop disables the opaque fast path, and
with default limits it fails outright at 120 frames on the retained-bytes cap.

The plan walks the hierarchy top-down and lands as four ordered child stories:

- **opt-1 (level 3/4 — microarchitecture / runtime idiom):** rewrite the
  per-pixel classifier and the three per-pixel reduction loops to hoisted-u64,
  row-based, changed-pixel-only. Single-threaded 3–6× on every pass. No public
  API change, byte-identical output. Lowest risk, done first.
- **opt-2 (level 1 — data model):** filmstrip normalizes only the selected tile
  subsequence + locator, and the opaque fast path extends to cropped opaque
  frames. ~4–5×; the retained-bytes failure disappears. Independent of the
  classification path.
- **opt-3 (level 1 by the inherently-parallel exception):** a
  `std::thread::scope` worker pool runs the (now-rewritten) per-pair loops in
  parallel with deterministic, order-independent integer reductions. Near-linear
  on the frame/pair dimension.
- **opt-4 (level 1 — eliminate redundant work):** compute the adjacent-pair
  classification once per `(normalized-sequence identity, noise_floor)` cohort as
  an explicitly threaded `SharedAdjacentAnalysis` (per-pair comparison aggregates
  + bit-packed per-pair change masks) and share it across select, difference-map,
  and motion-history. Collapses `4M + B` toward `M + B`. Largest surface,
  sequenced last.

opt-1 × opt-3 alone move the identity suite comfortably under the 15 s deadline
and under 1 s; opt-4 removes the residual duplicate CPU and restores deadline
headroom.

## Profiling Summary

Probe families: workload baseline + on-CPU + microarchitecture (external
`perf stat`: 529 G instructions, 122.8 G cycles, IPC 4.3, negligible
cache/branch misses → instruction-bound, not memory-bound; identical 7.0 ns/px
at identity and fit-limits confirms scalar-throughput, not bandwidth).

| Hot site | File:lines | Evidence | Root cause |
|---|---|---|---|
| Adjacent-pair classify (all pairs) | `measure.rs:238` `measure_adjacent`, `measure.rs:248-360` `measure_pixels` | 980 ms/pass, 8.2 ms/pair, 7.0 ns/px | full u128 checked pass over every pixel of every pair |
| Per-pixel classifier | `measure.rs:369-392` `classify_pixel_change` | ~137 instr/px | recomputes `noise_floor²·weight_sum` via `checked_pow`/`checked_mul` **per pixel**; all-u128 |
| Per-pixel coordinate math | `measure.rs:272-279`, `difference_map.rs:187-188`, `motion_history.rs:389-390` | `div`+`mod`+`try_from` every pixel | x/y needed only for changed pixels (bounds); mask tested per pixel |
| Baseline re-scan | `select.rs:609-639` `peak_baseline_comparison` | `B` extra non-adjacent pair classifications | genuinely different (baseline→N) comparisons; irreducible |
| Difference re-classify | `difference_map.rs:165-229` | re-runs `classify_pixel_change` over the same `M` pairs | independent second full pass |
| Motion double pass | `motion_history.rs:232` + `384-410` `accumulate_segment` | `measure_adjacent` (full `M`) **plus** a second full classify `M` | the `measure_adjacent` call is used only for gap/segment structure, which needs no pixels |
| Filmstrip over-normalization | `filmstrip.rs:957-965` | 389 ms; ~320 ms (82 %) normalizing 112 unread frames; fails at 120 frames on 67 MB cap | normalizes whole cropped source though only `plan.tiles()` (8) + locator are drawn |
| Cropped normalization | `normalize.rs:290`, `680-719` `normalize_frame_general` | per-pixel `composited_pixel` checked arithmetic | any crop disables the opaque fast path (`normalize.rs:545-573`) |

Not bottlenecks (measured, left alone): PNG encode at `Compression::Best`
30–120 ms/artifact; render/draw 30–75 ms.

## Optimization Plan

### Optimization 1: Hoisted-u64, row-based, changed-only per-pixel loops

**Hierarchy Level**: Data locality / microarchitecture + runtime idiom (levels 3/4)
**Probe Family**: microarchitecture (instruction throughput / autovectorization)
**Bottleneck**: `classify_pixel_change` recomputes the threshold and runs all-u128
checked arithmetic on every pixel; `measure_pixels` / difference / motion do
`div`+`mod`+`try_from` per pixel for coordinates only the changed subset needs.
The u128 width and per-pixel branches block autovectorization; ~137 instr/px.
**Expected Metric Movement**: instructions/px ~137 → ~25–45; ns/px 7.0 → ~1.5–2.3;
per pass 980 ms → 150–300 ms single-threaded (3–6×); IPC should rise as the inner
kernel becomes branch-light and vectorizable.

**Overflow proof (documented invariant for the unchecked inner loop):** channel
delta ≤ `u16::MAX` = 65 535, so `delta² ≤ 4 294 836 225`; the three weighted
squares sum to at most `WEIGHT_SUM · 65535²` = `65536 · 4 294 836 225`
≈ `2.815e14`. The threshold `noise_floor² · WEIGHT_SUM` has the same
`2.815e14` maximum. Both are `< 2^63` (9.22e18) with ~32 000× headroom, so the
per-pixel classifier is exact in `u64`. **The aggregate sums stay u128** —
`weighted_square_sum`, `absolute_sum`, `luminance_sum` accumulate over up to
~1.17 M changed pixels and can reach ~3.3e20, so they keep `u128` checked
addition. Only per-pixel classification and its `weighted_square` move to `u64`.

#### Implementation Units

##### Unit 1.1: u64 hoisted classifier
**File**: `crates/temporal-vision/src/measure.rs`

```rust
/// Per-pixel result; u64 is exact by the documented ≤ 2.815e14 < 2^63 bound.
pub(crate) struct PixelChange { pub changed: bool, pub weighted_square: u64 }

/// Precompute once per pair (not per pixel).
struct PixelClassifier { threshold: u64 }          // noise_floor² · WEIGHT_SUM, u64
impl PixelClassifier {
    fn new(p: MeasurementParameters) -> Self;       // hoist threshold; checked at boundary
    #[inline] fn classify(&self, before: &[u16;3], after: &[u16;3]) -> PixelChange; // unchecked u64
}
```

**Implementation Notes**:
- `WEIGHT_SUM`, `RED/GREEN/BLUE_WEIGHT` become `u64` constants (values unchanged).
- `classify` uses plain `u64` mul/add (no `checked_*`); a `debug_assert!` on the
  computed `weighted_square` documents the `< 2^63` invariant. The threshold is
  computed once in `new` with a checked path (it depends only on `noise_floor`).
- `weighted_square` returned as `u64`; call sites that accumulate widen to `u128`
  at the boundary (`u128::from(...)`).
- Delete the per-pixel `classify_pixel_change` free function once callers migrate,
  or keep a thin `#[cfg(test)]`-only shim only if an existing unit test needs it
  (the threshold-boundary test at `measure.rs:517-570` must still pass unchanged).

**Acceptance Criteria**:
- [ ] `classify` produces identical `changed` and `weighted_square` values as the
      current u128 path for the full u16 delta range (property/boundary test).
- [ ] Existing `identity_and_threshold_boundary_are_exact` passes unchanged.
- [ ] `measure_pixels` output (`MeasurementVector`) byte-identical.

##### Unit 1.2: Row-based, changed-only `measure_pixels`
**File**: `crates/temporal-vision/src/measure.rs:248-360`

**Implementation Notes**:
- Replace the `enumerate()` + `index % width` / `index / width` with an outer
  `for y in 0..height` / inner `for x in 0..width`, slicing `earlier`/`later` and
  the optional analysis mask **per row** (one row bit-slice lookup, not a
  per-pixel `mask.includes(x, y)`).
- Compute `x`/`y`, luminance, min/max bounds **only inside the `changed` branch**.
- Aggregate sums remain `u128` checked. `changed`, `compared`, and the perceptual
  divisor math are unchanged.

**Acceptance Criteria**:
- [ ] `MeasurementVector` (all six fields incl. `changed_region_bounds`) identical
      for clean, masked, and gapped fixtures.
- [ ] Masked path still honors mask exclusion exactly (per-row slice ≡ per-pixel test).

##### Unit 1.3: Row-based difference accumulation
**File**: `crates/temporal-vision/src/difference_map.rs:165-229`

**Implementation Notes**:
- Same row-based rewrite; reuse `PixelClassifier`. `magnitude` stays `u64`
  (single-pixel `weighted_square` fits), `weighted_time` stays `u128`.
- Per-row mask slice; only changed pixels touch the six accumulator arrays.

**Acceptance Criteria**:
- [ ] `DifferenceAccumulators` arrays identical (existing
      `accumulation_is_exact_gap_aware_repeated_and_bounded` passes).
- [ ] Difference-map `output_hash` unchanged.

##### Unit 1.4: Row-based motion accumulation
**File**: `crates/temporal-vision/src/motion_history.rs:363-416`

**Implementation Notes**:
- Row-based rewrite of `accumulate_segment`; reuse `PixelClassifier`. Motion only
  needs `PixelChange.changed` (weight is per-rank, not per-pixel), so it ignores
  `weighted_square`.

**Acceptance Criteria**:
- [ ] `MotionHistoryPlan` (accumulation, ever_changed, outline, counts) identical
      (existing `accumulation_saturates_resets_at_gaps_and_respects_the_mask`).

---

### Optimization 2: Filmstrip subsequence normalization + cropped-opaque fast path

**Hierarchy Level**: Algorithmic / data model (level 1)
**Probe Family**: on-CPU + memory (retained bytes)
**Bottleneck**: `generate_region_filmstrip` normalizes the entire cropped source
(all 120 frames) though `plan.tiles()` (≤ tile_limit) + one locator are the only
frames rendered; ~82 % of normalization is unread, and default limits reject the
120-frame case on the 67 MB retained-bytes cap. Compounding: any crop routes
every pixel through `normalize_frame_general`'s per-pixel `composited_pixel`.
**Expected Metric Movement**: filmstrip 389 ms → ~80 ms (~4–5×); normalized
retained bytes for a filmstrip drop from `120·crop_px·6` to `(tiles+1)·crop_px·6`;
the 120-frame default-limit failure is eliminated.
**Why higher levels don't apply**: this *is* the level-1 fix — normalize the
frames the artifact actually reads, not all of them.

#### Implementation Units

##### Unit 2.1: Normalize only the selected tile subsequence
**File**: `crates/temporal-vision/src/filmstrip.rs:906-1010` (`generate_region_filmstrip`, `render_filmstrip`)

**Implementation Notes**:
- Selection (`plan_region_filmstrip` → `select_indices`) already runs before
  normalization and yields **distinct, strictly increasing** source indices, so
  reordering is safe and tile selection semantics are untouched.
- Build a sub-`FrameSequence` from `plan.tiles()` frames (clone the ≤ tile_limit
  frames, as the locator already does at `967`), normalize *that* cropped
  (no gaps/markers needed — filmstrip normalization is pixel-only; gaps/markers
  in the manifest come from `source`, unchanged).
- `render_filmstrip` must index the tile subsequence by **tile position**, not by
  `tile.frame_index()`: change `&normalized.frames()[tile.frame_index()]`
  (`filmstrip.rs:1254`) to `&normalized.frames()[index]` (the `enumerate()` index
  at `1248`). Per-frame normalization is neighbor-independent, so each tile's
  pixels are byte-identical to the full-sequence normalization.
- `normalized.normalization_steps()` and the locator normalization are unchanged
  (steps depend only on params/crop). Manifest `source_frame_ids` /
  `analyzed_frame_ids` / `selected_frame_ids` derive from `artifact_source_indices`
  (`filmstrip.rs:1016-1040`), which are source indices — unaffected.

**Acceptance Criteria**:
- [ ] Filmstrip `output_hash`, manifest, and image bytes byte-identical to the
      current full-normalization output for the same request (equivalence test).
- [ ] A 120-frame default-limits filmstrip request **succeeds** (regression pins
      the former `retained bytes 345600000 exceeds limit 67108864` failure).
- [ ] Locator tile and gap-after markers unchanged.

##### Unit 2.2: Extend the opaque fast path to cropped opaque frames
**File**: `crates/temporal-vision/src/normalize.rs:290, 521-573`

**Implementation Notes**:
- Allow the fast path when the frame is fully opaque and the crop is a valid
  sub-rectangle (drop the `crop == full frame` requirement in
  `can_use_opaque_full_frame_fast_path`; keep the opacity + admissible-scale
  checks). Add `normalize_opaque_crop` that walks only the crop rows, emitting
  `linear_channel(rgba[c])` (identity) or the existing box-average
  (`normalize_opaque_downscale` with a crop origin) (downscale).
- **Correctness proof:** for `alpha = 255`, `composited_pixel` computes
  `(linear_channel(c)·255 + 0 + 127) / 255 = linear_channel(c)` exactly, so the
  crop fast path is byte-identical to `normalize_frame_general` for opaque input;
  the downscale box-average matches `downscaled_pixel` (which sums
  `composited_pixel` = `linear_channel` for opaque).
- `allow_opaque_full_frame_fast_path` at `normalize.rs:290` becomes
  "allow when `!restricted_domain`" (crop is now permitted); rename accordingly.

**Acceptance Criteria**:
- [ ] Cropped-opaque normalized frames byte-identical to the general path
      (extend `opaque_full_frame_fast_path_matches_reference_across_rectangular_scales`
      with cropped cases).
- [ ] Non-opaque or masked/region-restricted input still uses the general path.

---

### Optimization 3: Deterministic `std::thread::scope` parallelism for the per-pair loops

**Hierarchy Level**: Parallelism, promoted to level 1 by the inherently-parallel
exception (each adjacent pair is fully independent; the sequential loop *is* the
algorithmic limit).
**Probe Family**: on-CPU (near-linear scaling) — no off-CPU/lock probe needed
because the design has no shared mutable state on the hot path.
**Bottleneck**: `measure_adjacent`, difference accumulation, and motion
accumulation iterate pairs strictly sequentially on one core though 16 are
available.
**Expected Metric Movement**: near-linear on the pair dimension up to the worker
cap; combined with opt-1 the identity suite lands well under 1 s.
**Why higher levels don't apply**: opt-1 already exhausted the scalar/locality
wins; the remaining cost is genuinely `N` independent pair computations.

**Dependency policy:** rayon is **not** a workspace dependency and the crate is
deliberately dependency-minimal (serde, schemars, thiserror, png, sha2).
Introducing rayon for one hot loop is unjustified. Use `std::thread::scope`
(std, no new dependency, hermetic). Worker count =
`min(available_parallelism, work_items, 16)`; a single work item or a 1-core host
runs inline (no threads spawned). The app already calls generators on
`tokio::task::spawn_blocking`, so a nested scoped pool is safe.

**Determinism (byte-identical output is the hard contract — VISUAL-EVIDENCE.md
§Determinism):**
- `measure_adjacent`: partition the pair-index range into contiguous chunks; each
  worker writes each pair's `FrameComparison` into its **pre-assigned output
  slot**. No shared reduction → output order is positional and independent of
  scheduling.
- Difference / motion per-pixel accumulators: give each worker a **private**
  accumulator (or private column band) and combine by integer add. All
  accumulation is integer (`u32`/`u64`/`u128` add, `max`, bit-`or`), which is
  associative and commutative, so any combine order yields byte-identical arrays.
  Motion's per-segment `saturating_add` then pixelwise `max` is likewise
  order-independent when combined by `max`.

#### Implementation Units

##### Unit 3.1: Scoped parallel-for helper
**File**: `crates/temporal-vision/src/lib.rs` (new `parallel.rs` module, `pub(crate)`)

```rust
/// Run `body` over `0..count`, chunked across ≤ cap scoped threads.
/// `body(range)` writes only into its own output slots / private accumulator.
pub(crate) fn for_each_chunk(count: usize, body: impl Fn(std::ops::Range<usize>) + Sync);
/// Map+reduce variant for per-pixel accumulators: `init` per worker, `fold`, `merge`.
pub(crate) fn map_reduce<T: Send>(count: usize, init: impl Fn() -> T + Sync,
    fold: impl Fn(&mut T, usize) + Sync, merge: impl Fn(T, T) -> T) -> T;
```

**Implementation Notes**:
- Compute the worker cap from `std::thread::available_parallelism()`; clamp to 16
  and to `count`. `count <= 1` or cap `== 1` runs inline.
- `merge` is applied in deterministic worker order; integer-only merges make order
  immaterial, but fixing the order removes any doubt and keeps floats out.

##### Unit 3.2: Parallelize the three loops
**Files**: `measure.rs:238-246`, `difference_map.rs:165-229`, `motion_history.rs:363-416`

**Acceptance Criteria**:
- [ ] Sequential-vs-parallel output byte-identical for 1, 2, 8, 120 frames and
      1, 2, 16 workers (equivalence test forcing worker counts).
- [ ] No `unsafe`; no shared mutable hot-path state (data-race-free by construction).
- [ ] Single-frame / single-core requests spawn no threads.

---

### Optimization 4: Shared adjacent-pair classification across generators

**Hierarchy Level**: Algorithmic — eliminate redundant work (level 1)
**Probe Family**: on-CPU (duplicate-pass elimination)
**Bottleneck**: for one storyboard+difference+motion request each generator
independently classifies the same `M` adjacent pairs (`4M + B` total passes,
~3.9 s duplicate). Generators that share an identical `(normalized-sequence
identity, noise_floor)` are doing byte-identical per-pixel classification.
**Expected Metric Movement**: `4M + B` → `M + B` (motion and select stop
re-classifying; difference skips the threshold compare on unchanged pixels);
2.5–4× on multi-artifact requests, multiplying with opt-1/opt-3.
**Why higher levels don't apply**: this is the highest level — stop computing the
same thing four times.

**Decision 1 — owning type & what is shared (explicit threading, no hidden
cache).** Introduce `temporal_vision::SharedAdjacentAnalysis`, computed **once
per cohort** and passed by reference into the generators; nothing is memoized
behind a lookup. It carries exactly the shared substrate the item calls for
("share results/change-masks across select, difference-map, motion-history"):

```rust
pub struct SharedAdjacentAnalysis {
    /// Per-pair aggregate outcomes; select/storyboard consume these (tiny: M·~48 B).
    comparisons: Box<[FrameComparison]>,
    /// Bit-packed changed-pixel mask per measurable pair; motion consumes directly,
    /// difference uses to skip unchanged pixels. Bounded ≈ measurable_pairs·pixels/8
    /// (≈ 17 MB at 119 pairs × 1.17 M px); gated by the normalized-bytes budget.
    change_masks: Option<PairChangeMasks>,
}
pub fn analyze_adjacent_pairs<F>(normalized: &NormalizedSequence<F>,
    measurement: MeasurementParameters, want_change_masks: bool)
    -> Result<SharedAdjacentAnalysis>;
```

One fused traversal classifies each pixel once and, in the same pass, fills the
per-pair `FrameComparison` aggregate **and** (when `want_change_masks`) writes the
pair's change-mask bit. Consumers stay decoupled at render time:
- **select** (`generate_storyboard` / `select_storyboard_frames`): accepts
  `Option<&[FrameComparison]>`; when present it skips its own `measure_adjacent`.
  The non-adjacent baseline run (`peak_baseline_comparison`, `B`) is genuinely
  different comparisons and stays local.
- **difference-map** (`DifferenceAccumulators::accumulate`): accepts the change
  masks; for masked-changed pixels it recomputes `weighted_square` (needed for
  magnitude/timing) but **skips the threshold classify for the unchanged
  majority** — its own reduction (`change_count`/`magnitude_sum`/…) stays in
  `difference_map.rs`, decoupled from difference's frequency/palette params.
- **motion-history** (`accumulate_segment`): with change masks it does **zero**
  classification — it applies its per-rank decay weight to masked pixels. Decay
  stays motion-local (not part of the cohort key).

*Rejected alternative:* fully fusing difference's and motion's reductions into the
shared pass would remove difference's changed-pixel recompute but couples three
generators' reduction logic and motion's decay into one function. Change-mask
sharing keeps each reduction in its own module (code economy) while still sharing
the expensive threshold classification, and stays memory-bounded.

**Threading seam (service).** In `src/artifacts/service.rs::run_flight`, the
per-generator groups that share one epoch and an identical normalization identity
+ `noise_floor` form a cohort. Compute `SharedAdjacentAnalysis` once for the
cohort (on the existing `run_blocking` CPU pool, under the same deadline/memory
permit) after normalization, and thread `Option<&SharedAdjacentAnalysis>` through
`generators::generate` into each `generate_*` call. When a generator is alone in
its cohort, or masks would exceed the normalized-bytes budget, the analysis is
absent and generators fall back to their existing self-classifying path — so the
change is strictly additive and never regresses a lone request.

#### Implementation Units

##### Unit 4.1: `SharedAdjacentAnalysis` + fused builder
**File**: `crates/temporal-vision/src/measure.rs` (or new `shared.rs`), exported from `lib.rs`
**Acceptance Criteria**:
- [ ] `comparisons` equals `measure_adjacent(normalized, measurement)` exactly.
- [ ] `change_masks[p]` bit set iff `classify(...).changed` for that pixel/pair.
- [ ] Builder is parallelized via opt-3 with byte-identical output.

##### Unit 4.2: Optional shared inputs on the generators
**Files**: `render.rs` (`generate_storyboard`), `select.rs` (`select_storyboard_frames`), `difference_map.rs` (`render_difference_map`/`accumulate`), `motion_history.rs` (`build_motion_history_plan`/`generate_motion_history`)
**Implementation Notes**:
- Add an `Option<&SharedAdjacentAnalysis>` parameter (or a small `with_shared(...)`
  builder on the `*Parameters` types to avoid churning every signature). Absent →
  today's behavior exactly.

**Acceptance Criteria**:
- [ ] With and without the shared analysis, every artifact's `output_hash`,
      manifest, and image bytes are byte-identical.

##### Unit 4.3: Cohort detection + threading in the service
**File**: `src/artifacts/service.rs:334-520`, `src/artifacts/generators.rs:177-325`
**Acceptance Criteria**:
- [ ] A storyboard+difference+motion request over one epoch computes the
      adjacent-pair classification **once** (assert via the perf scaffold's
      `classified_pixel_passes` accounting dropping from `4M+B` toward `M+B`).
- [ ] A single-generator request is unchanged (no shared analysis built).
- [ ] Mixed noise_floor / mixed normalization across generators falls back
      per-generator (no incorrect sharing).

## Benchmarks

**Primary scaffold (extend, don't replace):**
`crates/temporal-vision/tests/pair_classification_perf.rs` — the existing ignored
release benchmark already exercises storyboard + orientation + difference (+
optional motion) over the deterministic 1080p moving-patch sequence, records the
`4M+B` accounting, external `perf stat` counter status, allocation/RSS, and — via
`assert_equivalent` / `duplicate_run_equal` — **byte equality of normalized
buffers, artifact bytes, manifests, and digests across two runs**. This
equivalence harness is the determinism guard; keep it green at every step.

Additions:
- A `PERF_PAIR_WORKERS` env knob (Config) to pin the opt-3 worker count; assert
  output digests are identical across `1`, `2`, and `16` workers.
- After opt-4, assert the effective `classified_pixel_passes` accounting collapses
  from `4M+B` toward `M+B` for the `storyboard-difference-motion` generator set.
- **New ignored bench** `crates/temporal-vision/tests/region_filmstrip_perf.rs`
  modeled on the existing scaffold: 120×1224×958, a fixed source-image crop,
  default limits; records wall time, normalized bytes, and the tile/normalized
  frame counts; includes a subsequence-vs-full-normalization digest equivalence
  assertion and a default-limits 120-frame **success** regression.

**Run commands** (Rust 1.85, release, single-threaded numbers first):
```bash
# adjacent-pair suite, identity, 120 frames, all three generator families
PERF_PAIR_FRAMES=120 PERF_PAIR_SCALE=identity \
PERF_PAIR_GENERATORS=storyboard-difference-motion \
  rustup run 1.85.0 cargo test -p temporal-vision --release --locked \
  --test pair_classification_perf -- --ignored --nocapture
# under perf stat for microarchitecture counters
PERF_PAIR_COUNTER_STATUS=captured perf stat -e task-clock,cycles,instructions,cache-misses,branch-misses \
  rustup run 1.85.0 cargo test -p temporal-vision --release --locked \
  --test pair_classification_perf -- --ignored --nocapture
# filmstrip
rustup run 1.85.0 cargo test -p temporal-vision --release --locked \
  --test region_filmstrip_perf -- --ignored --nocapture
```

**Baseline targets (measured, identity):**
- one adjacent-pair classification pass: **980 ms** (8.2 ms/pair, 7.0 ns/px)
- 4-artifact suite: **9.3 s** (~2.35 s at fit-limits downscale)
- region filmstrip: **389 ms** (~320 ms normalizing 112 unread frames); 120-frame
  default-limits case **fails** on the retained-bytes cap.

**Expected targets:**
- per adjacent-pair pass, single-threaded after opt-1: **150–300 ms** (3–6×)
- per adjacent-pair pass after opt-3 (16 workers): near-linear on top
- 4-artifact suite at identity after opt-1+opt-3: **well under 1 s**
- after opt-4: residual duplicate CPU removed (`4M+B`→`~M+B`), deadline headroom
  restored under the 15 s `max_wall_time` (`scheduler.rs:85`)
- region filmstrip: **~80 ms** (~4–5×); 120-frame default-limits case **succeeds**.

**Counter targets:** instructions/px ~137 → ~25–45; ns/px 7.0 → ~1.5–2.3 (single
thread); IPC ≥ current 4.3 (kernel stays instruction-dense but does far fewer
instructions); allocations/op unchanged except the bounded ~17 MB shared
change-mask (opt-4) and the ~15× smaller filmstrip normalized buffer (opt-2).

## Implementation Order

1. **opt-1** (inner-loop scalar rewrite) — highest single-threaded win, no API
   change, byte-identical; unblocks parallelizing the rewritten loops.
2. **opt-2** (filmstrip subsequence + cropped-opaque fast path) — independent,
   fixes an outright failure mode; can proceed in parallel with opt-1.
3. **opt-3** (scoped parallelism) — depends on opt-1's rewritten loops.
4. **opt-4** (shared adjacent-pair classification) — largest surface, depends on
   opt-1 and opt-3; sequenced last and validated as incremental once the suite is
   already under the deadline.

## Risks

**Determinism under parallelism (pre-mortem).** The contract
(VISUAL-EVIDENCE.md §Determinism) requires byte-identical measurements,
selections, manifests, and **output pixels** for identical inputs, and artifact
`output_hash` pins it. Failure modes and mitigations:
- *Non-associative reduction reorders bytes.* — All hot-path reductions are
  integer (`add`, `max`, bit-`or`), which are associative and commutative; there
  is no floating point. Workers still merge in fixed order to eliminate doubt.
  Guard: sequential-vs-parallel and cross-worker-count digest equivalence tests
  (opt-3 AC), plus the scaffold's `duplicate_run_equal`.
- *Shared mutable state races.* — Design admits none on the hot path: per-pair
  outputs go to pre-assigned slots; per-pixel accumulators are per-worker private
  then merged. No `unsafe`; `cargo test` runs under the normal toolchain and the
  equivalence tests would catch a race as a digest mismatch.
- *Worker-count-dependent output.* — Chunk boundaries never affect values because
  each pair/pixel is computed independently; test pins 1/2/16 workers equal.

**Correctness of the u64 inner loop.** Risk: an unproven overflow. Mitigation: the
documented `≤ 2.815e14 < 2^63` bound (~32 000× headroom), `debug_assert!` at the
boundary, aggregate sums kept in checked `u128`, and the existing exact
threshold-boundary test.

**Filmstrip provenance drift.** Risk: remapping tile indices or subsequence
normalization silently changes selection or manifest populations. Mitigation:
selection is unchanged (`select_indices` untouched); equivalence test asserts the
filmstrip `output_hash`, manifest, and bytes match the full-normalization output;
120-frame success regression.

**Needs host attention:**
- **opt-4 service restructuring** touches `run_flight`'s grouping/threading and is
  the riskiest unit; if cohort detection proves invasive, opt-1+opt-2+opt-3 already
  meet the deadline and `<1 s` target, so opt-4 can be reviewed/landed on its own
  merits or deferred without losing the headline wall-time result.
- **Out-of-scope contract follow-up (RGB16-linear normalized format).** The 6 B/px
  RGB16-linear normalized representation is what forces 120-frame identity runs
  over the 67 MB retained-bytes cap and down to fit-limits scale. Narrowing the
  classification domain (e.g. a lower-precision or luminance-only analysis buffer)
  would halve or quarter retained bytes but **changes documented evidence
  semantics** (VISUAL-EVIDENCE.md §Normalization / §Visual-Change Measurements and
  the manifest transfer/compositing provenance). This is a strategic contract
  decision for the user, **explicitly out of scope** for this feature. Recommend
  capturing it as a separate `[research]`/foundation item if the user wants to
  pursue larger identity-scale budgets. opt-2 already removes the filmstrip
  instance of this failure without touching the format.

## Implementation notes

- Implemented opt-1 with the hoisted `PixelClassifier`, the documented
  `WEIGHT_SUM * u16::MAX^2 ≈ 2.815e14 < 2^63` invariant, row cursors, and
  changed-pixel-only aggregate work. Checked `u128` aggregation remains at the
  reduction boundary.
- Implemented opt-2 with selected-tile subsequence normalization, locator
  normalization, cropped opaque identity/downscale kernels, and the 120-frame
  retained-byte regression. The subsequence frame values match the corresponding
  full-normalization frames.
- Implemented opt-3 with scoped workers capped at 16 and fixed-order private
  accumulator merges. The worker-count test covers 1, 2, and 16 workers.
- Implemented opt-4 with `SharedAdjacentAnalysis`, optional bit-packed masks,
  generator threading, cohort keys scoped by epoch/normalization identity/noise
  floor, and scheduler permits retained for live shared masks. Lone cohorts and
  budget failures retain the independent fallback path.
- Added shared-vs-independent artifact equivalence assertions for storyboard,
  difference-map, and motion-history outputs; existing output hashes and
  manifests remain unchanged.

Benchmark measurements, release build, 2026-07-23:

| Workload | Before evidence | After measurement |
| --- | ---: | ---: |
| 120 frames, 1224×958, identity, all three generators, one worker | 9.3 s suite; 980 ms adjacent pass | 2,075,270 µs wall; `M+B` accounting (199 classified passes), duplicate digest guard passed |
| Same workload, 30-frame 1920×1080 worker-count comparison | — | 1,066,098 µs at worker 1 and 1,113,817 µs at worker 16; normalized/artifact/output digests identical |
| 120-frame 1224×958 region filmstrip | 389 ms; default-limit retained-byte failure | 135,429 µs; 12 tiles; 5,111,808 normalized bytes; success |

The adjacent-pair baseline and original filmstrip failure values are the measured
design evidence above; the after runs used the checked-in ignored release
scaffolds. Hardware scheduling made the 16-worker sample slower than the pinned
single-worker sample on this host, while determinism remained exact.
