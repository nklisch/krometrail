# Setup and activation

Plugin installation, binary installation, MCP activation, and tool discovery are separate checks.
Do not report Krometrail ready until each required layer is observed.

## 1. Install the native plugin

Use one marketplace, not both.

### First-party Krometrail marketplace

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

A successful plugin install means the harness has the skill and MCP declaration. It does not mean the
`krometrail` executable exists.

## 2. Verify or install the binary

Check first:

```bash
command -v krometrail
krometrail --version
krometrail --help
```

If it is missing and the user has asked to set up Krometrail, use the canonical installer:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

The installer selects the platform asset, verifies `checksums.txt`, executes the candidate's exact
`--version` identity before replacement, and installs atomically. It supports Linux and macOS on x64
and arm64. Windows uses the published `.exe` asset directly.

If installation was not part of the user's request, explain the missing binary and ask before running
networked installation. Do not substitute a JavaScript-package fallback: the current product is the Rust binary.

The default destination is `~/.local/bin/krometrail`. If the current agent process does not inherit a
new shell profile, either restart the harness or add the directory to that process before verifying:

```bash
export PATH="$HOME/.local/bin:$PATH"
krometrail --version
```

## 3. Activate and verify MCP

The plugin declares this stdio server:

```json
{
  "command": "krometrail",
  "args": ["mcp"]
}
```

Restart or reload the harness after installing the binary or changing plugin state. Confirm that the
Krometrail tool list includes browser lifecycle, observation/control, and temporal evidence tools,
and that the `temporal-artifact` and `temporal-source-frame` resource templates are available. Do not
run `krometrail mcp` in an ordinary terminal as a health command; it is a stdio protocol server
and waits for an MCP client.

If native plugin MCP loading is unavailable, configure the same direct command through the harness's
normal MCP lifecycle rather than copying plugin cache files:

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

Use each command's `--help` as the authority if the installed harness version differs.

## Troubleshooting boundaries

- **Skill visible, tools absent:** inspect plugin details and MCP status; the binary may be missing from
  the harness process's `PATH`, or the harness may need a restart.
- **Server exits immediately:** run `krometrail --version` and `krometrail doctor` outside MCP. Preserve
  the exact error rather than replacing the direct command with a downloader.
- **Browser not found:** install a supported local Chrome/Chromium build or pass an explicit executable
  through `start_browser` as advertised by its tool schema.
- **Attach fails:** use an explicitly configured local Chromium debugging endpoint. Do not expose or
  assume a remote endpoint.
- **Tools present but actions require approval:** this plugin intentionally does not auto-allow every
  browser-control tool. Follow operator and harness permission policy.

## Update and removal

Update the marketplace and plugin with native commands:

```bash
# Claude Code
claude plugin marketplace update krometrail
claude plugin update krometrail@krometrail

# Codex: refresh the catalog, then reinstall if a newer plugin is available
codex plugin marketplace upgrade krometrail
codex plugin remove krometrail@krometrail
codex plugin add krometrail@krometrail
```

For the sibling marketplace, substitute `nklisch-skills` as the marketplace name.

Remove native state with the matching lifecycle:

```bash
claude plugin uninstall krometrail@krometrail --scope user
claude plugin marketplace remove krometrail --scope user

codex plugin remove krometrail@krometrail
codex plugin marketplace remove krometrail
```

Plugin removal does not remove the independently installed binary or retained local Krometrail data.
Remove those only when the user explicitly requests it and after identifying how the binary was
installed.
