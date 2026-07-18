---
id: idea-bound-compact-snapshot
created: 2026-07-18
updated: 2026-07-18
tags: []
---

During the post-1.1.0 cross-surface manual pass, a routine `fill` on Wikipedia with the default response projection returned a very large accessibility snapshot alongside the concise interaction result. The projection omitted inline pixels and reported omitted nodes, but the remaining node slice was still large enough to dominate agent context. Reproduce by starting a managed session at `https://en.wikipedia.org/wiki/Browser_automation`, resolving the `Search Wikipedia` searchbox with `query_page`, and filling the returned reference without a `response` override. Revisit the compact snapshot's default node/detail budget so ordinary mutations remain genuinely ergonomic on large documents while canonical/full drill-down stays explicit.
