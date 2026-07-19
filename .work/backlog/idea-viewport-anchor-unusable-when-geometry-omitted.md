---
id: idea-viewport-anchor-unusable-when-geometry-omitted
created: 2026-07-19
updated: 2026-07-19
tags: [agent-ux, browser]
---

Found in the 2026-07-19 third shakedown: `snapshot_page {"anchor":"viewport"}`
degrades to document-order ranking on any page whose DOMSnapshot node table
exceeds MAX_SNAPSHOT_NODES (5000), because viewport ranking needs the geometry
pass and that page omits geometry (`geometry_omitted: true`). Verified on
Wikipedia "Web browser" (~8000 nodes, scrolled to y=2600): the viewport-anchored
snapshot returned the same top-of-document nav items as document anchor, honestly
flagged with geometry_omitted. The degradation is correct and explicit — but the
practical consequence is that viewport anchoring is only functional on the small
pages where it is least needed, and unavailable on exactly the large pages where
"what is actionable on screen right now" matters most. This is the mirror image
of `idea-scroll-geometry-node-cap` (now fixed): the scroll path degrades
gracefully, but the viewport-ranking feature built alongside it has no signal to
work with on the same large pages.

Fix direction: allow a bounded viewport-scoped geometry acquisition even when the
full-document node count exceeds the cap — acquire DOMSnapshot rects only for
nodes intersecting the current visual viewport (a much smaller set) rather than
the whole document, so viewport ranking survives on large pages. Alternatively,
raise or make configurable the geometry node cap when the anchor is explicitly
viewport (the caller opted into the cost).
