---
id: feature-perf-artifact-pixel-pipeline
kind: feature
stage: drafting
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
