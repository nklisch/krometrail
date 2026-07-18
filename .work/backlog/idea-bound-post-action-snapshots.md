---
id: idea-bound-post-action-snapshots
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux]
---

# Bound post-action snapshots on complex pages

During manual testing with Krometrail v1.0.3, `create_page` succeeded on Wikipedia but returned a 403-node post-action accessibility snapshot. The structured response expanded to roughly 48,000 tokens, creating substantial agent-context pressure even though the caller primarily needed the new target identity and current page state. Preserve useful live evidence while bounding or progressively exposing large post-action snapshots on complex public pages.
