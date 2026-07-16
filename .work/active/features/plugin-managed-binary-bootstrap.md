---
id: plugin-managed-binary-bootstrap
kind: feature
stage: drafting
tags: [distribution, mcp, security]
parent: null
depends_on: [agent-plugin-distribution]
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Bootstrap and update the plugin-managed binary

## Brief

Make a native Krometrail plugin installation sufficient to start the MCP server without a separately preinstalled `krometrail` executable. The plugin carries a small launcher and checksum-verifying managed installer. On first MCP activation it installs the exact Krometrail release matching the plugin version into persistent user-owned plugin data, then executes that binary. On a later plugin version, the same launcher automatically installs and selects the matching binary before MCP starts.

This is release-coupled update behavior, not an unconstrained background updater: a plugin never polls or installs an arbitrary `latest` binary. Claude Code may update a third-party marketplace automatically only when the operator enables its native auto-update setting; Codex follows its native marketplace refresh/update lifecycle. Once either harness activates a newer plugin, binary synchronization is automatic and deterministic.

## Strategic decisions

- **Activation boundary:** bootstrap from the MCP launcher because neither native marketplace offers one portable trusted post-install hook, while plugin MCP servers start automatically when enabled.
- **Version authority:** the root Cargo package version remains authoritative. Plugin manifests, catalogs, and the launcher version file are derived release metadata updated transactionally by the release helper.
- **Update policy:** install exactly the plugin version. Do not query `latest`, cross major versions, silently downgrade a system binary, or reuse an unrelated executable from `PATH`.
- **Storage:** use persistent plugin data when the harness exports it and a private XDG data fallback otherwise. Keep versioned binaries so existing sessions continue using the release they started with.
- **Security:** download only bounded HTTPS GitHub release/checksum artifacts, validate redirect hosts, require one exact checksum entry, verify executable identity before atomic publication, reject symlinked or non-user-owned destinations, and reserve stdout for MCP JSON-RPC.
- **Supported hosts:** automatic bootstrap covers the supported Linux/macOS x64/arm64 environments. Windows remains a best-effort direct-download environment with an explicit unsupported bootstrap error.

## Simplification opportunity

Replace the PATH-dependent MCP declaration and manual binary prerequisite with one package-owned launcher. Keep the public standalone installer for direct CLI users; do not add a second Rust updater command or background daemon.
