---
id: agent-plugin-distribution-marketplace-publication
kind: story
stage: done
tags: [distribution]
parent: agent-plugin-distribution
depends_on: [agent-plugin-distribution-canonical-package]
release_binding: 1.0.1
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

## Implementation notes

- Added separate first-party Claude and Codex catalogs; each native CLI registered the local marketplace and installed Krometrail 1.0.0 into an isolated home.
- Updated the sibling Claude entry to a current versioned `git-subdir` pointer and added a native Codex catalog with explicit source objects for all existing entries.
- Kept Krometrail package content exclusively in this repository; `../skills/plugins/krometrail` does not exist.
- Published Krometrail first, then verified both sibling native catalogs resolved the canonical remote plugin as 1.0.0. `nklisch/skills` PR #43 merged as `a57f2a2`; fresh remote Claude and Codex installs discovered the direct MCP declaration and shared skill.
