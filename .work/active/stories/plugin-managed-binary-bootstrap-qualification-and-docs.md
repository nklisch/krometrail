---
id: plugin-managed-binary-bootstrap-qualification-and-docs
kind: story
stage: implementing
tags: [distribution, testing, documentation]
parent: plugin-managed-binary-bootstrap
depends_on: [plugin-managed-binary-bootstrap-launcher-and-installer, plugin-managed-binary-bootstrap-release-version-sync]
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Qualify managed bootstrap and update behavior

Add hermetic launcher/installer fixtures and upgrade the native Claude/Codex smoke so plugin activation—not a pre-seeded PATH binary—installs and serves Krometrail. Document exact plugin-coupled updates, Claude's operator-controlled marketplace auto-update setting, Codex's native update lifecycle, standalone binary updates, offline behavior, and per-harness managed-data cleanup.

## Acceptance evidence

- Cold, warm/offline, version-change, concurrent, and failure-preservation fixtures pass without network.
- Native installed declarations bootstrap MCP and expose expected tools/resources in isolated homes.
- Documentation makes no claim that installing marketplace metadata alone has already downloaded a binary.
- Update and uninstall behavior are explicit for Claude, Codex, and direct standalone installations.
