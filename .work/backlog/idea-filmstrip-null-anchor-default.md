---
id: idea-filmstrip-null-anchor-default
created: 2026-07-22
updated: 2026-07-22
tags: [temporal, agent-ux]
---

`generate_region_filmstrip` with the optional `anchor` omitted fails with
"filmstrip anchor lies outside the source range". Repro (v1.5.0 shakedown):
a valid `range_handle` + `viewport_css` region + `source_frame_id` from the
range's first frame, no `anchor` → error; the identical request with an
explicit in-range anchor succeeds. A null anchor should default to something
inside the resolved range (range start, or the source frame's session time)
instead of an out-of-range sentinel; alternatively make `anchor` required in
the schema so the contract is explicit.
