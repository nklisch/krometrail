---
id: story-analysis-budget-tuning
kind: story
stage: done
tags: [perf, temporal]
parent: null
depends_on: []
release_binding: 1.6.2
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Analysis and video budget tuning

## Brief

The 2026-07-23 v1.6.1 shakedown hit two analysis ceilings on the first
realistic try: a 5.2 s full-rate interaction window (430 frames at 1224×958)
exceeded both the exhaustive-analysis cap (120 frames / 768 MiB decoded,
`ArtifactWorkLimits::default()` in `src/artifacts/scheduler.rs:66`) and the
temporal-video source-frame cap (`MAX_VIDEO_SOURCE_FRAMES: 120`,
`crates/krometrail-core/src/video/plan.rs:15`) despite being well under the
30 s video duration cap. The byte-side limits are the earned memory envelope;
the 120-frame ceilings are chosen values that bind first at normal viewport
frame sizes (~4.7 MiB decoded each → the 768 MiB budget alone would admit
~163 frames). User direction: double the 768 MiB cap and tune the surrounding
limits so realistic windows fit, with benchmark evidence.

## Direction

- `ArtifactWorkLimits::default()`: `max_decoded_bytes` 768 MiB → 1536 MiB
  (explicit user direction). Preserve `validate()` invariants: raise
  `max_combined_request_bytes` (1 GiB → 2 GiB, still < u32::MAX) and revisit
  `max_normalized_bytes` proportionally if it becomes the next binding
  constraint. `max_source_frames` 120 → 240 so the frame ceiling tracks the
  doubled byte budget at normal frame sizes; the per-plan effective limit
  stays `min(frames, decoded_bytes / per_frame)`.
- Temporal video: raise `MAX_VIDEO_SOURCE_FRAMES` so a full-rate clip of a
  realistic window fits (the shakedown's 430-frame / 5.2 s clip must
  generate). Choose the value with benchmark evidence against
  `VideoGenerationLimits.max_wall_time` (30 s) and the encoded-input budget;
  do not raise the 30 s source-duration or output-geometry caps.
- Benchmark evidence, not vibes: use the existing perf harness
  (`src/artifacts/overlap_perf.rs` and any video benches) plus a measured
  end-to-end run — exhaustive difference_map at the new frame ceiling and a
  430-frame video — recording wall time and peak memory against
  `max_wall_time` (15 s artifacts / 30 s video) with margin. Record before/
  after numbers in this story body. If a proposed value cannot meet wall-time
  with margin, pick the largest value that does and record why.
- The uniform_bounded sampling density doubles implicitly with
  `max_source_frames`; confirm bounded difference_map/motion_history at 240
  analyzed frames stays within wall-time on the benchmark machine.
- Related backlog: `perf-scout-share-pair-classification` benchmarks at
  8/30/60/120 frames — extend its matrix note to the new ceiling if touched;
  do not take over that item.

## Acceptance criteria

- [ ] New limits in place with `validate()` invariants intact; limit-error
      messages reflect the new numbers where they appear in pinned tests.
- [ ] The shakedown reproduction passes: exhaustive difference_map over ≤240
      frames of 1224×958 succeeds; a 430-frame / ~5 s temporal video
      generates within wall-time.
- [ ] Before/after benchmark numbers recorded in this story body.
- [ ] Full workspace gate green.

## Implementation notes

- Selected limits: `ArtifactWorkLimits::default()` now permits 240 source
  frames, 1536 MiB decoded bytes, and a 2 GiB combined request; the 512 MiB
  normalized-byte limit remains because it was not the binding constraint.
  `MAX_VIDEO_SOURCE_FRAMES` is 480. The 30 s source-duration and output
  geometry caps are unchanged.
- On this machine, the existing exhaustive `difference_map` overlap harness
  (one sequential permit, one repetition) measured 120 frames at 5.010488 s,
  with a 1,740,096 KiB peak RSS delta, before the change. At 240 frames it
  measured 5.728628 s and a 1,886,812 KiB peak RSS delta, leaving about
  9.27 s under the 15 s artifact wall-time limit. The harness exercised the
  full difference-map workload; peak RSS includes process and harness
  overhead in addition to the scheduler's decoded-byte budget.
- The production video service benchmark uses 430 full-rate frames at
  1224×958 over about 5.2 s, with the bounded 640×502 output request. The
  pre-change 120-frame run measured 496 ms wall time and 24,368 KiB peak RSS;
  the post-change 430-frame run measured 2,598 ms and 62,728 KiB peak RSS.
  This leaves about 27.4 s under the 30 s video wall-time limit, so 480 is
  retained as the cap with margin for the 430-frame shakedown. The benchmark
  required exact balanced PTS lookup and an explicit single-threaded
  ultrafast H.264 policy; its argument identity is now `h264-v2`.
- Limit-message tests and schemas use the current values directly; no old
  limit aliases or compatibility paths were retained.

## Review

Bounded fresh-context review: PASS with one accepted minor: the ignore-gated 430-frame video perf test measures and reports wall time without asserting a ceiling; it is a measurement harness backing the recorded numbers, not a regression guard. PTS filter equivalence and balanced-lookup boundary handling verified by hand; ultrafast preset affects compression only, and the h264-v2 policy identity correctly invalidates v1-cached clips.
