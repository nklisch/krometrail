---
id: agent-plugin-distribution-canonical-package
kind: story
stage: done
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

## Implementation notes

- Replaced the legacy plugin metadata with version-aligned Claude and Codex manifests.
- Added one direct `krometrail mcp` stdio declaration; installation remains explicit and separate.
- Added one 105-line portable skill plus focused evidence and setup references and Codex picker metadata.
- Preserved empty default permissions so installation does not silently authorize every browser-control operation.
- Verified with `claude plugin validate`, the Codex skill validator, JSON contract checks, stale-contract search, and `git diff --check`.
