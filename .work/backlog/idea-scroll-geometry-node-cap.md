---
id: idea-scroll-geometry-node-cap
created: 2026-07-19
updated: 2026-07-19
tags: [bug, browser, agent-ux]
---

Found in the 2026-07-19 post-fix live shakedown (dev build of main at v1.2.3-19,
Wikipedia "Web browser" article, 8362 DOM nodes): every `scroll` and `set_viewport`
on a page whose DOMSnapshot node table exceeds 5000 nodes returns `status: degraded`
with the entire post-action snapshot unavailable — `page_observation_failed`:
"selected DOM semantic acquisition contains 8362 nodes, exceeding the 5000-node
limit; narrow the query to a smaller document". A standalone `snapshot_page` on the
same page succeeds (431 presentation omissions), so the failure is specific to the
viewport-anchoring geometry pass added by
`feature-response-evidence-economy-viewport-anchoring`.

Root cause: the geometry decode in `crates/krometrail-cdp/src/control/snapshot.rs`
(~line 846) hard-errors when `DOMSnapshot.captureSnapshot` returns more than
`MAX_SNAPSHOT_NODES` (5000), while the accessibility acquisition path (~line 514)
handles the same pressure by omitting nodes and reporting the omission. The caller
treats the geometry error as the whole live-observation snapshot failing, so exactly
the long pages viewport anchoring was designed for lose all structured post-scroll
evidence (the staleness auto-image is the only surviving signal).

Secondary defects in the same path:
- Recovery text is query-oriented ("narrow the query to a smaller document",
  `retry: safe`) but the operation is a scroll — there is no query to narrow and
  retry is deterministic futility.
- Fix direction: fall back to the plain accessibility projection (no
  `document_rect` anchoring) when the geometry pass exceeds its bound, keeping the
  operation `succeeded` with an explicit anchoring-omitted note — bounded-loss
  accounting instead of hard failure.
