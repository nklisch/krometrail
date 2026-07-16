---
id: plugin-managed-binary-bootstrap-qualification-and-docs
kind: story
stage: done
tags: [distribution, testing, documentation]
parent: plugin-managed-binary-bootstrap
depends_on: [plugin-managed-binary-bootstrap-launcher-and-installer, plugin-managed-binary-bootstrap-release-version-sync]
release_binding: 1.0.1
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

## Implementation notes

- Added hermetic fault fixtures for every security/lifecycle branch and updated the static plugin contract for both native MCP schemas and Cargo-derived version identity.
- Reworked the opt-in native smoke so no standalone binary is preinstalled: Claude cold activation bootstraps from its resolved plugin data, and Codex cold activation bootstraps from its resolved package `cwd` and XDG fallback.
- Native qualification verifies the exact v1.0.0 identity, representative tools, both resource templates, skill discovery, uninstall behavior, Claude-owned data cleanup, and Codex fallback data persistence without invoking a model or Chrome.
- Updated README, installation/development guides, agent navigation, plugin setup guidance, stable compatibility policy, changelog, and generated `llms-full.txt`, including operator-controlled Claude auto-update and explicit Codex lifecycle behavior.
- Passed native Claude/Codex lifecycle qualification, plugin validators, docs build, shell/static contracts, and the complete locked Rust fmt/check/test/clippy gate.
- Added a warm-path regression proving that even a version-correct binary behind a symlink is rejected before its `--version` or MCP entry point can execute; native and distribution suites pass after the review fix.
