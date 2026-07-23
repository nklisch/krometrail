---
id: story-filmstrip-anchor-default
kind: story
stage: done
tags: [temporal, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Filmstrip null anchor defaults inside the range

## Brief

`generate_region_filmstrip` with the optional `anchor` omitted fails with
"filmstrip anchor lies outside the source range". Repro (v1.5.0 shakedown):
a valid `range_handle` + `viewport_css` region + `source_frame_id` from the
range's first frame, no `anchor` → error; the identical request with an
explicit in-range anchor succeeds. A null anchor should default to something
inside the resolved range (range start, or the source frame's session time)
instead of an out-of-range sentinel; alternatively make `anchor` required in
the schema so the contract is explicit.

## Acceptance

- Omitting `anchor` on a valid filmstrip request succeeds with a
  deterministic documented default (inside the resolved range).
- An explicit out-of-range anchor still fails with the current explicit
  error.
- A test covers the omitted-anchor path end to end.

## Implementation notes

- Root cause: the wire deserializer used the resolved range's semantic anchor
  as the omitted value. That anchor is valid for the resolved interval, but a
  retained visual epoch can start later than the interval and temporal-vision
  correctly rejects an anchor outside its actual first/last frame range. The
  region-filmstrip path had no epoch-time materialization; storyboard's
  existing epoch clamping hid the same latent mismatch.
- Chosen default: an omitted region-filmstrip anchor is finalized to the
  declared source frame's session time, which is deterministic and inside the
  visual source sequence. An omitted storyboard anchor is finalized to the
  first retained source frame for the same source-safe policy. Explicit
  anchors remain unchanged and retain out-of-range rejection.
- Regression coverage: added
  `progressive::tests::omitted_region_filmstrip_anchor_uses_source_frame_time_end_to_end`
  plus `explicit_anchor_outside_source_range_stays_rejected`; the existing core
  explicit-anchor rejection coverage remains in place.
- Files changed: core artifact/progressive request validation, artifact
  generator preparation and service plumbing, progressive region/service tests,
  and the end-to-end progressive regression test.
