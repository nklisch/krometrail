---
id: feature-viewport-scoped-geometry
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Viewport-scoped geometry for large-page snapshots

## Brief

`snapshot_page {"anchor":"viewport"}` degrades to document-order ranking on any
page whose DOMSnapshot node table exceeds `MAX_SNAPSHOT_NODES` (5000), because
viewport ranking needs the geometry pass and such pages omit geometry
(`geometry_omitted: true`). Verified on Wikipedia "Web browser" (~8000 nodes,
scrolled to y=2600): the viewport-anchored snapshot returned the same
top-of-document nav items as document anchor, honestly flagged with
`geometry_omitted`. The degradation is correct and explicit — but the practical
consequence is that viewport anchoring only functions on small pages where it
is least needed, and is unavailable on exactly the large pages where "what is
actionable on screen right now" matters most.

Fix direction from the shakedown: keep geometry available for viewport ranking
on large pages by bounding the geometry that is actually used, not by refusing
the pass — e.g. acquire/retain DOMSnapshot rects only for nodes intersecting
the current visual viewport (a much smaller set) when the full-document node
count exceeds the cap, or allow the explicit `anchor: viewport` request to opt
into the larger geometry cost. The existing honest `geometry_omitted` signal
stays for whatever remains genuinely unavailable.

## Simplification opportunity

None identified beyond reusing the existing geometry plumbing; the change
should bound an existing pass rather than adding a parallel one.

Origin: `.work/backlog/idea-viewport-anchor-unusable-when-geometry-omitted.md`
(2026-07-19 third shakedown).
