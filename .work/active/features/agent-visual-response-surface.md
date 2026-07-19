---
id: agent-visual-response-surface
kind: feature
stage: drafting
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Make the default agent surface visual, discoverable, and bounded

Correct the MCP presentation defects reproduced during comparative manual testing: chronological browser-event detail is projected away, all resolved-range follow-up schemas appear as opaque object unions, concise ranges repeat every frame UUID, and sanitized URL digests dominate compact inventories. Make visual operations include one useful image by default while retaining an explicit `inline_images: false` text-only override, and remove non-actionable root-document entries from the concise action target index.

## Source findings

- `idea-expose-browser-event-detail`
- `idea-compact-temporal-frame-ids`
- `idea-compact-sanitized-url-digests`
- Direct manual-test finding: retained image resources were produced but not visually inspected because omitted `inline_images` suppressed every pixel.
- Direct manual-test finding: `RootWebArea` can occupy prime concise target space despite not being a meaningful interaction target.

## Simplification opportunity

Keep one canonical result and one response projector. Treat `inline_images` as an optional override whose omitted value materializes from the operation kind, compact resolved-range and URL identity only in concise presentation, and generate concrete range-or-handle schema branches rather than maintaining opaque constraint-only unions.
