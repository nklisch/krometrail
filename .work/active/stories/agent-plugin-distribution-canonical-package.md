---
id: agent-plugin-distribution-canonical-package
kind: story
stage: implementing
tags: [distribution, mcp, documentation]
parent: agent-plugin-distribution
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Build the canonical Claude and Codex plugin package

Replace the stale plugin shell with sibling native manifests, one direct stdio MCP declaration, and one complete portable `krometrail` skill. The skill is an evidence-literacy and setup guide: it follows the user's debugging direction, maps questions to available evidence, explains source-derived limitations, and links to focused references instead of prescribing a fixed workflow or copying generated schemas.

## Acceptance evidence

- Both native manifests and the MCP declaration pass their static/native validators.
- Skill validation passes and no historical DAP, npm fallback, or `chrome_*` contract remains.
- Setup and evidence references cover the design's full evidence and activation boundaries.
