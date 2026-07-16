---
id: plugin-managed-binary-bootstrap
kind: feature
stage: implementing
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

## Architectural choice

Use a POSIX launcher at `plugin/bin/krometrail` as the only MCP command. The launcher reads the exact release from `plugin/version`, checks the versioned managed binary, invokes a bundled hardened installer only when that exact binary is absent or invalid, and then `exec`s it. It emits bootstrap diagnostics to stderr only so the child owns stdout from the first MCP byte onward.

Claude and Codex use separate native MCP files because their safe path authorities differ:

```json
// Claude: placeholder substitution and persistent plugin data are native.
{"mcpServers":{"krometrail":{"command":"${CLAUDE_PLUGIN_ROOT}/bin/krometrail","args":["mcp"],"env":{"KROMETRAIL_MANAGED_ROOT":"${CLAUDE_PLUGIN_DATA}"}}}}

// Codex: cwd is normalized against the installed plugin root.
{"krometrail":{"command":"sh","args":["bin/krometrail","mcp"],"cwd":"."}}
```

The launcher falls back to `${XDG_DATA_HOME:-$HOME/.local/share}/krometrail/plugin` when no harness data directory is exported. Each release lives under `versions/<semver>/krometrail`; this avoids replacing an executable used by an older live session. Direct standalone installs remain independent.

This approach was chosen over a SessionStart hook, which may run after MCP startup and requires separate Codex trust; over a downloader embedded in `.mcp.json`, which is opaque and hard to test; and over `latest` polling, which can mismatch plugin guidance/configuration or cross a major-version boundary.

## Implementation units

### 1. Managed binary launcher and installer

**Files:** `plugin/bin/krometrail`, `plugin/scripts/install-managed.sh`, `plugin/version`, `plugin/.mcp.json`, `plugin/.mcp.codex.json`, native plugin manifests

**Story:** `plugin-managed-binary-bootstrap-launcher-and-installer`

The installer accepts no caller-selected version or destination: both come from package-controlled inputs. It maps supported host/architecture pairs to stable release assets; performs bounded manual HTTPS redirects through an allowlist; rejects malformed or duplicate checksum entries; validates private user-owned non-symlink directories; executes the candidate's exact `--version`; and atomically publishes with private permissions. Concurrent installers may race only by publishing independently verified identical artifacts.

**Acceptance criteria:**
- [ ] A plugin with no system `krometrail` installs and starts its exact managed release on first MCP activation.
- [ ] A matching managed binary starts with no network access or mutation.
- [ ] A changed plugin version installs/selects that version without replacing an older version directory or touching a standalone binary.
- [ ] Failed downloads, checksums, identity checks, unsafe paths, unsupported hosts, and missing prerequisites fail before MCP execution and preserve prior versions.
- [ ] Bootstrap emits no stdout; successful execution presents the normal MCP 2025-06-18 tools/resources.
- [ ] Claude uses native root/data placeholders; Codex loads the direct-map config and resolves its relative cwd inside the plugin.

### 2. Release-version synchronization

**Files:** `scripts/bump-version.ts`, `tests/distribution-static.sh`, plugin manifests/catalogs/version marker

**Story:** `plugin-managed-binary-bootstrap-release-version-sync`

Extend the Cargo-authoritative release transaction so a Krometrail bump derives all plugin version metadata from the root package version, validates each file began at the current value, rolls every derived file back if release checks fail, and stages them in the release commit. Non-Krometrail fixture repositories keep the helper's generic Cargo-only behavior.

**Acceptance criteria:**
- [ ] One patch prepare changes Cargo root/workspace/lock plus both manifests, both first-party catalog entries, and `plugin/version` to the same value.
- [ ] Dry-run changes nothing; a failed prepare restores every source and derived file.
- [ ] Release mode stages all version-bearing files before the immutable tag is created.
- [ ] Static contracts reject any Cargo/plugin/catalog/launcher version drift.

### 3. Lifecycle qualification and operator guidance

**Files:** `tests/plugin-bootstrap-fixtures.sh`, `tests/plugin-install-smoke.sh`, `tests/plugin-static.sh`, README and installation/development guidance, generated `docs/public/llms-full.txt`

**Story:** `plugin-managed-binary-bootstrap-qualification-and-docs`

Add hermetic installer/launcher fixtures for cold install, warm no-network start, version transition, concurrency, and failure preservation. The opt-in native smoke installs the plugin into isolated Claude/Codex homes, invokes each installed MCP declaration rather than a separately installed binary, and verifies tool/resource discovery. Documentation distinguishes automatic managed synchronization from native plugin marketplace update policy and standalone installer updates.

**Acceptance criteria:**
- [ ] Hermetic fixtures cover every material failure boundary without network, Chrome, or model calls.
- [ ] Isolated Claude and Codex plugin installs bootstrap their managed binary and complete MCP discovery with no binary pre-seeded on PATH.
- [ ] Claude auto-update is described as operator opt-in for third-party marketplaces; Codex update behavior uses its native supported lifecycle.
- [ ] Plugin uninstall/removal guidance states whether managed data is automatically deleted or requires explicit cleanup on each harness.
- [ ] Direct binary users retain the checksum-verifying installer and manual rerun update path.

## Implementation order

1. `plugin-managed-binary-bootstrap-launcher-and-installer`
2. `plugin-managed-binary-bootstrap-release-version-sync` depends on the package version marker.
3. `plugin-managed-binary-bootstrap-qualification-and-docs` depends on both runtime and release contracts.

## Simplification

- Replace the PATH-dependent MCP declaration and manual binary prerequisite with one package-owned launcher.
- Keep the public standalone installer for direct CLI users; do not add a second Rust updater command or background daemon.
- Remove plugin setup prose that asks agents to install the binary manually before MCP activation.

## Testing

- Hermetic shell fixtures protect download, path, version, concurrency, and stdout contracts.
- Real native CLI smoke protects the Claude/Codex package resolution and installed-config seams, but remains opt-in because those CLIs are external dependencies.
- Existing Rust MCP protocol tests remain authoritative after the launcher hands off.
- Release-helper fixtures protect transactional version derivation and rollback.

## Risks

- Automatic startup installation is a higher-trust network boundary. Exact version pinning, checksums, redirect allowlisting, identity execution, and private atomic publication are mandatory; no fallback may weaken them.
- Codex does not document MCP `PLUGIN_ROOT` substitution. Its config must rely only on the loader's tested plugin-relative `cwd` normalization.
- Offline first activation cannot succeed without a cached managed binary. The error must say that explicitly and direct users to the standalone installer or a later retry; it must not claim the plugin is ready.
