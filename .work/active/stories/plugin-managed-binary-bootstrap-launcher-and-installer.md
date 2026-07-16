---
id: plugin-managed-binary-bootstrap-launcher-and-installer
kind: story
stage: done
tags: [distribution, mcp, security]
parent: plugin-managed-binary-bootstrap
depends_on: []
release_binding: 1.0.1
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Add the plugin-managed launcher and release installer

Build the package-owned `krometrail` launcher, exact version marker, hardened managed release installer, and separate Claude/Codex MCP declarations. Cold activation installs the exact package release into private persistent data; warm activation performs no network work; a plugin version change selects a new versioned binary without altering standalone installations.

## Acceptance evidence

- Exact checksum and executable identity precede atomic publication.
- Redirect, size, path ownership, symlink, host/architecture, and prerequisite boundaries fail closed.
- Concurrent cold starts can publish only independently verified identical artifacts.
- stdout remains untouched until the managed MCP binary executes.
- Claude and Codex native config loaders resolve the launcher from the installed plugin package.

## Implementation notes

- Added a package-owned launcher, exact `plugin/version` authority projection, and versioned managed data layout.
- Added a bounded HTTPS installer with redirect host allowlisting, exact checksum cardinality, candidate identity execution, private user ownership, symlink rejection, and atomic publication.
- Split native MCP declarations: Claude uses `CLAUDE_PLUGIN_ROOT`/`CLAUDE_PLUGIN_DATA`; Codex uses its verified plugin-relative `cwd` normalization and a direct server map.
- Hermetic fixtures prove cold install, warm offline start, side-by-side version update, concurrent convergence, checksum failure preservation, symlink rejection, redirect rejection, and stdout isolation.
- Native Claude health loading installed v1.0.0 automatically in under one second; Codex listed the installed plugin-relative launcher and cwd exactly.
- Review hardening moved ownership, private-mode, parent-symlink, regular-file, and exact identity checks ahead of every warm execution through a no-network installer verification mode.
