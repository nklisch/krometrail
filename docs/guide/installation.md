---
title: Installation
description: Install the managed Claude Code or Codex plugin, or the standalone Krometrail binary.
---

# Installation

Krometrail supports two separate installation modes:

1. the native agent plugin, which contributes the skill, MCP declaration, and a private managed binary;
2. the optional standalone `krometrail` executable for terminal or manual MCP use.

The native plugin does not require the standalone command to be on `PATH`. Each layer remains independently observable and removable.

## Install the native agent plugin

Use one marketplace source.

### First-party marketplace

```bash
# Claude Code
claude plugin marketplace add nklisch/krometrail --scope user
claude plugin install krometrail@krometrail --scope user

# Codex
codex plugin marketplace add nklisch/krometrail
codex plugin add krometrail@krometrail
```

### nklisch skills marketplace

```bash
# Claude Code
claude plugin marketplace add nklisch/skills --scope user
claude plugin install krometrail@nklisch-skills --scope user

# Codex
codex plugin marketplace add nklisch/skills
codex plugin add krometrail@nklisch-skills
```

Both sources install the canonical package from Krometrail's `plugin/` directory. The sibling marketplace stores pointers, not a copied skill or manifest.

### Managed binary activation

The plugin declares one local stdio server through its package-owned launcher. On first MCP activation, that launcher downloads the exact release coupled to the plugin version, verifies the selected asset against the release checksum, executes its exact version identity, and publishes it atomically into private per-user plugin data. Progress and errors use stderr; stdout is reserved for MCP.

This cold activation needs HTTPS access to the GitHub release and asset hosts. Warm starts use the verified local binary directly and do not poll the network. Updating the plugin selects its matching binary on the next activation without modifying an independently installed CLI. Prior managed versions remain available for an offline plugin rollback.

Restart or reload Claude Code or Codex after installation or update. Confirm that:

- native plugin details show the Krometrail skill and MCP component;
- MCP status reports Krometrail connected;
- browser lifecycle, observation/control, temporal evidence, and browser-event tools are visible;
- the `temporal-artifact` and `temporal-source-frame` resource templates are available.

The plugin intentionally does not auto-allow all browser-control tools. Harness and operator permission policy remains authoritative.

## Install the standalone binary

Install this only when you also want the `krometrail` terminal command or need manual MCP configuration. The public POSIX installer supports Linux and macOS on x64 and arm64:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
krometrail --version
krometrail doctor
```

It selects the release asset for the current host, verifies it against `checksums.txt`, requires the temporary binary's exact `krometrail <selected-semver>` identity, and replaces an existing installation only after all checks pass. The legacy `v0.2.20` TypeScript/DAP cutoff remains blocked so an old runtime cannot be presented as a current Rust installation.

Linux release assets target `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`; they are statically linked against musl. Stable public asset names are:

- `krometrail-linux-x64`
- `krometrail-linux-arm64`
- `krometrail-darwin-x64`
- `krometrail-darwin-arm64`
- `krometrail-windows-x64.exe`

Windows is a best-effort direct-download artifact, not an installer-supported or supported development environment. Verify its matching checksum before use.

### Select a version or destination

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v1.0.0
KROMETRAIL_INSTALL_DIR="$HOME/bin" curl -fsSL https://krometrail.dev/install.sh | sh
```

The default destination is `~/.local/bin/krometrail`. Restart the shell or add that directory to the current process if it is not already on `PATH`.

A standalone binary can be registered directly when native plugin MCP loading is unavailable:

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

`krometrail mcp` is a protocol server, not an interactive health command; use `krometrail --version` and `krometrail doctor` for terminal checks.

## Update

Refresh native plugin state with the harness lifecycle:

```bash
# Claude Code
claude plugin marketplace update krometrail
claude plugin update krometrail@krometrail

# Codex
codex plugin marketplace upgrade krometrail
codex plugin remove krometrail@krometrail
codex plugin add krometrail@krometrail
```

Substitute `nklisch-skills` when that is the registered marketplace. Claude Code can also auto-update an operator-enabled marketplace and its installed plugins after startup; enable or disable that through Claude's plugin manager or marketplace settings. Krometrail itself does not change that setting. After a plugin update, the next MCP activation installs the exact new binary when needed. Codex updates remain explicit with the native commands above. The managed launcher performs no background poll or independent `latest` lookup.

The standalone binary installer remains safe to rerun and validates the new candidate before replacement:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

## Remove

```bash
# Claude Code
claude plugin uninstall krometrail@krometrail --scope user
claude plugin marketplace remove krometrail --scope user

# Codex
codex plugin remove krometrail@krometrail
codex plugin marketplace remove krometrail
```

Claude owns the lifecycle of its plugin data. Codex's fallback managed versions live under `${XDG_DATA_HOME:-$HOME/.local/share}/krometrail/plugin` and can remain after plugin removal to support offline rollback. Plugin removal never deletes an independently installed executable or retained local recordings. Remove managed versions, standalone binaries, or evidence only after identifying their exact paths and confirming the user wants them removed.

## Local development install

To build the current source with the locked Cargo graph and copy the host release binary to `~/.local/bin`:

```bash
bash scripts/dev-install.sh
```

Set `KROMETRAIL_INSTALL_DIR` to choose another destination. This development helper uses the host's native Cargo target; it is separate from the release workflow's reproducible musl Linux matrix.
