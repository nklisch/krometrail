---
id: idea-query-node-limit-large-pages
created: 2026-07-22
updated: 2026-07-22
tags: [bug]
---

`query_page` and semantic `wait` fail hard on large real-world pages while
`snapshot_page` handles the same page gracefully. Repro (v1.5.1 live shakedown,
https://en.wikipedia.org/wiki/Software_bug):

- Plain role query (`role: link`, exact name) → error
  "accessibility nodes: 6060 exceeds limit 5000".
- The same query with `container_text` → error
  "accessibility nodes: 7687 exceeds limit 5000" (the container-eligibility
  pass acquires even more of the tree, so it trips earlier on borderline pages).
- Semantic wait (`role: heading`, exact name, `present`) → same
  "6060 exceeds limit 5000" failure, so waits are unusable on these pages too.
- `snapshot_page` on the exact same page **succeeds**, reporting bounded output
  with explicit omission accounting (`presentation_targets: 495`,
  `source_nodes: 1060` omitted).

Impact: the ergonomic targeting path (`query_page`) and the new full-tree
semantic wait are both unavailable on any document whose accessibility tree
exceeds 5,000 nodes — large Wikipedia articles, documentation sites, dense
dashboards. The failure is explicit (good: fails closed, no silent truncation),
but there is no degraded path and the error message offers no recovery action
(e.g. "scope the query to a descendant reference" or "use snapshot_page
references"), unlike other failure boundaries which return structured recovery
guidance.

Fix directions to weigh at design time: (a) bound the query acquisition the
same way snapshots are bounded and report source-omission counts on the query
outcome so queries stay usable with declared incompleteness; (b) at minimum,
attach a recovery action to the limit error pointing at descendant `scope`
narrowing and snapshot-reference targeting; (c) check whether the semantic-wait
probe can reuse the bounded snapshot acquisition instead of the unbounded full
acquisition. Note the asymmetry violates the spirit of bounded-loss-accounting:
one surface accounts for loss, the sibling surface refuses outright.

Also observed nearby (weak evidence, may be transient accessibility-tree
state rather than a defect): on the Wikipedia Main Page after the Vue typeahead
mounts and swaps `#searchInput`, `query_page` `role: searchbox` and
`role: textbox` both returned `no_match` for the id-less
`.cdx-text-input__input` (type=search), while the identical `searchbox` query
resolves fine on article pages. Worth a look while in the area.
