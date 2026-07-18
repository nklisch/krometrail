---
id: story-fix-mp4-timescale-duration-rounding
kind: story
stage: done
tags: [bug, visual, infra, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Accept valid MP4 movie-timescale duration rounding

## Symptom

Generating a video from retained real-browser frames returned `video_encoding_failed` even though
FFmpeg exited successfully and produced an H.264 MP4 whose codec, dimensions, per-sample timeline,
track duration, and output bounds all matched the request.

## Root cause

`validate_duration` compares cross-multiplied duration values but uses `timescale` as its tolerance.
That happens to allow one microsecond when the track timescale is 1,000,000, but for an MP4 movie
header with timescale 1,000 it permits only 1 microsecond instead of the format's one-millisecond
tick. A valid 1,434,473-microsecond presentation therefore serialized as 1,435 movie ticks and was
rejected for its unavoidable 527-microsecond rounding difference.

## Fix approach

Allow at most one tick in the compared container timescale. In the cross-multiplied units used by
the validator, one tick is always 1,000,000; retain rejection beyond that boundary.

## Regression test

`crates/krometrail-ffmpeg/src/mp4.rs` asserts that a 1,000-timescale duration rounded within one tick
is accepted and the next tick beyond the expected duration is rejected. The test fails before the fix
against the exact values observed from the real browser-video reproduction.

## Implementation notes

- Execution capability: direct primary-agent implementation; the defect was isolated to one private
  validator helper and one regression test, so delegation would add no useful breadth or isolation.
- Changed `crates/krometrail-ffmpeg/src/mp4.rs` to compare the cross-multiplied duration difference
  against one tick (`1_000_000` in those units) instead of the unrelated numeric timescale value.
- The regression test failed before the fix and passes afterward. All FFmpeg crate targets pass, as
  do workspace formatting, check, serial full tests, and all-target Clippy.
- The original reproduction now generates a 227,476-byte H.264 MP4 at 560×330 from all 16 retained
  browser frames. `ffprobe` reports the requested 1.434473-second duration.
- Adjacent issues parked: none. The earlier five-second request correctly exceeded the configured
  decoded-sequence working-set limit and required a narrower retained interval; that is not this bug.

## Review (2026-07-18)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Bounded inline standalone-story review; no independent or cross-model reviewer ran. The
correctness and test lenses confirm the tolerance is one tick in the validator's cross-multiplied
units and that the next larger tick is rejected. The change is private, minimal, and root-causal.
Security and breaking-change lenses were inapplicable because no input, execution, authorization,
persisted-format, or public API boundary changed. Existing foundation assertions remain current.
