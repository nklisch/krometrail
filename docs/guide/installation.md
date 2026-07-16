---
title: Install Krometrail
description: Install the Krometrail plugin for Claude Code or Codex, or add the standalone command for manual MCP setup.
---

# Install Krometrail

Choose the path that matches how you want to use it:

| You want to… | Install |
| --- | --- |
| Let Claude Code or Codex manage Krometrail | The native plugin (recommended) |
| Run terminal checks or configure MCP yourself | The standalone `krometrail` command |

You do not need both. The plugin-managed binary and a standalone installation are separate and do not update or remove each other.

## Recommended: install the native plugin

The plugin gives your agent the Krometrail skill, a local MCP connection, and a private managed binary.

Use one marketplace source.

### Claude Code

```bash
claude plugin marketplace add nklisch/krometrail --scope user
claude plugin install krometrail@krometrail --scope user
```

### Codex

```bash
codex plugin marketplace add nklisch/krometrail
codex plugin add krometrail@krometrail
```

Restart or reload Claude Code or Codex after installation. Then confirm that:

- the Krometrail skill is available;
- MCP reports Krometrail as connected;
- browser-control and temporal-evidence tools are visible.

Your agent may ask for permission before launching or controlling a browser. Krometrail follows the harness and operator approval policy rather than auto-allowing every action.

### First activation

The first MCP activation downloads the exact Krometrail release paired with the plugin, verifies its checksum and version, and stores it in private per-user plugin data. That first start needs HTTPS access to GitHub release and asset hosts. Later starts of the same version use the verified local binary without an update check.

The managed binary is private to the plugin. It does not add `krometrail` to your shell's `PATH`.

### Alternate marketplace

If you already use the `nklisch/skills` marketplace, install the same canonical plugin from there instead:

```bash
# Claude Code
claude plugin marketplace add nklisch/skills --scope user
claude plugin install krometrail@nklisch-skills --scope user

# Codex
codex plugin marketplace add nklisch/skills
codex plugin add krometrail@nklisch-skills
```

Use one marketplace source, not both.

## Optional: install the standalone command

Install the command when you want terminal checks or manual MCP configuration. The installer supports Linux and macOS on x64 and arm64:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
krometrail --version
krometrail doctor
```

`krometrail doctor` discovers a local Chrome or compatible Chromium installation without launching it.

The default destination is `~/.local/bin/krometrail`. If your shell cannot find it, restart the shell or add the directory to the current session:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Choose a version or destination

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v1.0.1
KROMETRAIL_INSTALL_DIR="$HOME/bin" curl -fsSL https://krometrail.dev/install.sh | sh
```

The installer selects the release for the current host, verifies it against the published checksums, checks the binary's exact version, and replaces an existing installation only after those checks pass.

Windows is a best-effort direct-download `.exe` from [GitHub Releases](https://github.com/nklisch/krometrail/releases/latest). It is not supported by the one-line installer or as a development environment. Verify the matching checksum before use.

## Connect a standalone binary to your agent

Register the local MCP server when native plugin loading is unavailable or you deliberately prefer manual setup:

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

Your MCP client starts `krometrail mcp`; you do not run it as an interactive terminal command. Use `krometrail --version` and `krometrail doctor` for terminal checks.

See [Manual MCP configuration](mcp-configuration.md) for a JSON client entry.

## Try it

After restarting your agent, ask:

> Use Krometrail to open `http://localhost:3000`, describe the current page, and tell me whether the Krometrail browser session is recording.

Then continue with the [agent workflow and example prompts](using-krometrail.md).

## Update

### Plugin

```bash
# Claude Code
claude plugin marketplace update krometrail
claude plugin update krometrail@krometrail

# Codex
codex plugin marketplace upgrade krometrail
codex plugin remove krometrail@krometrail
codex plugin add krometrail@krometrail
```

Substitute `nklisch-skills` when that is your marketplace. After an update, the next MCP activation installs the new plugin's matching binary. Krometrail does not poll `latest` or alter a separately installed command.

### Standalone command

Rerun the installer. It validates the new candidate before replacing the existing binary:

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

Removing the plugin does not delete a separately installed command or retained browser evidence. Confirm exact paths and ownership before deleting managed versions, standalone binaries, or recordings.
