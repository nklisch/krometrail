---
title: Installation
description: Install the Krometrail binary and native Claude Code or Codex plugin.
---

# Installation

Krometrail has two independently observable installation layers:

1. the `krometrail` executable;
2. the optional native agent plugin, which contributes the skill and `krometrail mcp` declaration.

A plugin install does not imply that the executable is available. Verify both before reporting the agent integration ready.

## Install the binary

The public POSIX installer supports Linux and macOS on x64 and arm64:

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

## Activate and verify MCP

The plugin declares one local stdio server:

```json
{
  "command": "krometrail",
  "args": ["mcp"]
}
```

After the binary and plugin are installed, restart or reload Claude Code or Codex. Confirm that Krometrail browser lifecycle, observation/control, temporal evidence, and browser-event tools are visible, together with the `temporal-artifact` and `temporal-source-frame` resource templates. `krometrail mcp` is a protocol server, not an interactive health command; use `krometrail --version` and `krometrail doctor` for terminal checks.

The plugin intentionally does not auto-allow all browser-control tools. Harness and operator permission policy remains authoritative.

## Update

The binary installer is safe to rerun and validates the new candidate before replacement:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

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

Substitute `nklisch-skills` when that is the registered marketplace.

## Remove

```bash
# Claude Code
claude plugin uninstall krometrail@krometrail --scope user
claude plugin marketplace remove krometrail --scope user

# Codex
codex plugin remove krometrail@krometrail
codex plugin marketplace remove krometrail
```

Plugin removal does not delete the independently installed executable or retained local recordings. Remove those only after identifying the installation and data paths and confirming the user wants them removed.

## Local development install

To build the current source with the locked Cargo graph and copy the host release binary to `~/.local/bin`:

```bash
bash scripts/dev-install.sh
```

Set `KROMETRAIL_INSTALL_DIR` to choose another destination. This development helper uses the host's native Cargo target; it is separate from the release workflow's reproducible musl Linux matrix.
