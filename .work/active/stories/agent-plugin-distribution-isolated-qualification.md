---
id: agent-plugin-distribution-isolated-qualification
kind: story
stage: implementing
tags: [distribution, testing, documentation]
parent: agent-plugin-distribution
depends_on: [agent-plugin-distribution-canonical-package, agent-plugin-distribution-marketplace-publication]
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Qualify isolated plugin and binary lifecycles

Add static package contracts, opt-in native Claude/Codex install/remove smoke coverage, and operator documentation. Run every native lifecycle under temporary homes, keep binary installation distinct from plugin installation, verify the published checksum and exact binary identity, and confirm MCP/skill package discovery without invoking a model or launching Chrome.

## Acceptance evidence

- Distribution tests protect current manifests, catalogs, skill, MCP command, and sibling pointer ownership.
- A v1.0.0 installer run into a temporary directory reports and executes `krometrail 1.0.0` with `mcp` available.
- Claude and Codex install, expose, and remove the plugin from isolated state without touching the operator's actual home.
- README and development docs give current install, activation, verification, update, and removal commands and state their evidence boundaries.
