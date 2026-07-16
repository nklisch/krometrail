---
id: plugin-managed-binary-bootstrap-launcher-and-installer
kind: story
stage: implementing
tags: [distribution, mcp, security]
parent: plugin-managed-binary-bootstrap
depends_on: []
release_binding: null
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
