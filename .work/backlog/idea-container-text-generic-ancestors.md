---
id: idea-container-text-generic-ancestors
created: 2026-07-22
updated: 2026-07-22
tags: [browser, agent-ux]
---

`container_text` role queries silently fail on generic-div markup. The
qualifier only consults ancestors whose role is in `LOCAL_CONTAINER_ROLES`
(`listitem, row, cell, gridcell, group, article, region, label, labeltext` —
`crates/krometrail-cdp/src/control/snapshot.rs:60`), so a checkbox inside
`<div class="row"><input type="checkbox"><span>Buy milk</span></div>` never
qualifies: the div is role `generic`, the walk finds no eligible container,
and the query returns bare `no_match` even in `contains` mode. Repro during
the v1.5.0 shakedown: the skill doc's own example shape
(`role: checkbox, container_text: "Buy milk"`) returns `no_match` while a
plain `role: checkbox` query finds the node. Most real apps build rows from
styled divs (flexbox/Tailwind), so the ergonomic entry point misses exactly
the markup it was designed for, and the failure gives no hint that the
control exists but no eligible container ancestor was found.

Directions to consider: surface an explicit "matched controls exist but no
eligible container ancestor" outcome or hint; or extend eligibility with a
bounded generic-ancestor fallback; and align the skill doc's "nearest
matching ancestor's rendered text" wording with the actual allowlist rule.
