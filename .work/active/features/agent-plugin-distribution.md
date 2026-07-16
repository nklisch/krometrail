---
id: agent-plugin-distribution
kind: feature
stage: drafting
tags: [distribution, mcp, documentation]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Distribute Krometrail as a native agent plugin

## Brief

Publish one canonical Krometrail plugin for Claude Code and Codex. The plugin declares the local stdio MCP server, carries one portable skill that helps agents choose and interpret browser and temporal visual evidence, and provides an explicit verified binary bootstrap path when `krometrail` is not installed. The skill follows the user's stated debugging approach instead of imposing a mandatory sequence.

Krometrail's repository remains the plugin source of truth. Native Claude and Codex catalogs expose it directly, while the sibling `../skills` marketplace publishes remote pointers rather than copied manifests or skills. Installation must keep native plugin state, binary availability, MCP connectivity, and agent skill discovery as separately verified facts.

The distribution replaces the obsolete pre-Rust plugin identity: no DAP claims, `chrome_*` namespace, npm fallback, or legacy debugging skills survive in the current package.

## Strategic decisions

- **Guidance style:** one evidence-literacy skill, not a prescribed debugging workflow — agents should match evidence to the user's question and current task.
- **Harnesses:** native Claude Code and Codex manifests/catalogs share the same complete skill and MCP declaration.
- **Binary setup:** explicit agent-invocable use of the checksum-verifying release installer — native plugin installation is not treated as executable installation, and no undocumented post-install hook or MCP-startup downloader is introduced.
- **Publication authority:** canonical assets live in this repository; `../skills` contains only marketplace pointers.
- **Permissions:** the plugin does not silently auto-allow every browser-control tool; operator and harness policy remain authoritative.

## Simplification opportunity

Replace the stale `plugin/` metadata and empty settings-only shell with one current package. Consolidate the old `krometrail-chrome`, `krometrail-debug`, and `krometrail-mcp` concepts into one skill because the Rust product has one browser/temporal MCP surface and no DAP runtime.
