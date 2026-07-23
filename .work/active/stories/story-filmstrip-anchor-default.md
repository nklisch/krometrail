---
id: story-filmstrip-anchor-default
kind: story
stage: implementing
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
