---
id: story-video-limit-structured-refusal
kind: story
stage: done
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

## Implementation notes

- Execution capability: inline implementation; the request validation and MCP routing changes share one small boundary.
- Review weight: standard default; no independent review requested.
- Files changed: `crates/krometrail-core/src/video/generation.rs`, `crates/krometrail-core/src/video/mod.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-core/src/video/tests.rs`, and `crates/krometrail-mcp/src/registry.rs`.
- Tests added: `temporal_video_limit_refusals_name_frame_and_duration_values` and `temporal_video_limit_refusal_stays_structured_while_shape_errors_keep_schema_text`; they pin numeric frame/duration refusals, recovery/retry, and the normal failed-tool surface while preserving schema errors for malformed shapes.
- Simplification: video MCP decoding now parses the existing generated wire shape once and invokes the domain constructor directly, avoiding a validation error being re-framed as schema mismatch.
- Discrepancies from design: no static checked-in MCP schema artifact exists; runtime schemas remain generated from the same wire type and the schema guard passes.
- Adjacent issues parked: none.

## Review

Bounded fresh-context review: PASS, no findings. Per-limit numeric refusals verified (including duration formatting), constructor-only validation confirmed with no unvalidated path via the range-handle route, and shape errors retained the schema-mismatch surface.
