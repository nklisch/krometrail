---
id: agent-plugin-distribution-marketplace-publication
kind: story
stage: implementing
tags: [distribution]
parent: agent-plugin-distribution
depends_on: [agent-plugin-distribution-canonical-package]
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Publish native Krometrail marketplace entries

Add first-party Claude and Codex marketplace catalogs to Krometrail and update the sibling `../skills` publisher to reference the canonical `plugin/` subdirectory with current browser-control and temporal-evidence metadata. Add a native Codex sibling catalog with explicit source objects while preserving all existing plugin entries.

## Acceptance evidence

- Both harnesses discover Krometrail from the first-party repository and sibling marketplace.
- The sibling contains pointers only, not copied Krometrail package content.
- Descriptions, categories, tags, versions, and paths agree with the canonical package and contain no DAP-era claims.
