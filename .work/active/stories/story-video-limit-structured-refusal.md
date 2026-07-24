---
id: story-video-limit-structured-refusal
kind: story
stage: implementing
tags: [temporal, mcp]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Structured temporal-video limit refusal

## Brief

`generate_temporal_video` refusals are the worst error on the current surface.
A 430-frame / 5.2s range produced:

> tool arguments do not match the advertised input schema at $:
> resource_limit_exceeded: temporal video request exceeds the fixed duration,
> frame, geometry, or output limit

Two defects (2026-07-23 v1.6.1 shakedown):

1. The message names neither the specific limit exceeded nor any numbers. The
   check at `crates/krometrail-core/src/video/generation.rs:54` collapses five
   distinct limits (source duration, source frames, width, height, encoded
   output bytes) into one static string. Contrast the exemplary artifact
   exhaustive-cap error shipped in 1.6.1: "exhaustive analysis source plan: 430
   frames and 2016858240 decoded bytes exceeds limit 120 frames and 805306368
   decoded bytes" + both remedies + retry: never.
2. The refusal is wrapped as a schema mismatch because validation runs inside
   the wire `Deserialize` (`deserialize_validated`), so the MCP layer prefixes
   "tool arguments do not match the advertised input schema at $". A
   resource-limit refusal is not an argument-shape problem.

## Direction

- Report the actually-violated limit(s) with observed vs limit values, in the
  same voice as the artifact exhaustive-cap error (e.g. "temporal video source
  plan: 430 frames over 5.227 s exceeds limit 240 frames" — exact wording per
  the checked limit; if several are violated, name each).
- Recovery text names the concrete remedies for the violated limit: narrow the
  resolved range / split into consecutive clips for frame- or duration-cap
  violations; lower output geometry/bytes for output-cap violations.
  `RetryAdvice::Never` (deterministic refusal).
- Stop surfacing this as a schema mismatch: the domain refusal must reach the
  caller as a normal failed tool response with the structured error (not the
  "$: does not match advertised input schema" wrapper). Keep genuine
  shape/serde failures in the schema-mismatch path.
- Wire schemas stay green (`bash scripts/check-wire-enum-schemas.sh`);
  regenerate if any published schema text changes.

## Acceptance criteria

- [ ] A frame-cap refusal names observed frame count and the cap; a
      duration-cap refusal names observed duration and the cap; output-cap
      refusals name the offending output value(s) and cap(s).
- [ ] The refusal arrives as a failed tool response with structured error
      (code, message, recovery, retry), not a schema-mismatch wrapper.
- [ ] Tests pin at least the frame-cap and duration-cap messages and the
      non-schema-mismatch surface.
- [ ] Full workspace gate green.
