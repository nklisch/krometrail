---
id: idea-semantic-wait-nonactionable
created: 2026-07-22
updated: 2026-07-22
tags: [browser, agent-ux]
---

Semantic waits cannot match non-actionable content, and the failure mode is
a bare timeout. `wait {condition: semantic}` shares `query_page`'s matching,
which returns only actionable references — so waiting for a toast, status
banner, or alert (`role: status` / `role: alert` content divs, arguably the
headline use case for a semantic wait) never matches and times out with no
hint that the role can never satisfy the query. Repro (v1.5.0 shakedown): a
`role=status` div added to the DOM with visible text; `wait semantic
{role: status}` times out while a `text` wait for the same content
satisfies immediately. Semantic waits work correctly for actionable targets
(verified with `role: button` + name).

Directions to consider: extend semantic-wait matching to the non-actionable
accessibility tree (snapshot-style, not query_page-style); or fail fast /
warn when the queried role is one the actionable matcher can never return;
and document the actionable-only scope in the wait schema description
either way.
