# Setup and activation

Plugin installation, managed-binary activation, MCP connection, and tool discovery are separately
observable checks. Do not report Krometrail ready until the layers needed for the task are observed.

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

The plugin contains the skill, MCP declaration, and a managed launcher. On its first MCP activation,
the launcher downloads the exact Krometrail release declared by that plugin version, verifies the
published checksum and executable identity, and stores it in private per-user plugin data. This cold
start needs network access. Later starts use the verified local binary without a network request.

Installing or updating the plugin is the operator's consent boundary for this release-coupled managed
binary. Merely loading this skill must not run a separate installer or modify a standalone installation.

## 2. Activate and verify MCP

Restart or reload the harness after installing or updating the plugin. The native declaration starts
the package-owned launcher; it does not depend on `krometrail` being on `PATH`.

Confirm all relevant layers:

- native plugin details show the Krometrail skill and MCP component;
- MCP status reports the Krometrail server connected;
- the tool list includes browser lifecycle, observation/control, and temporal evidence tools;
- `temporal-artifact`, `temporal-artifact-manifest`, and `temporal-source-frame` resource templates
  are available.

Temporal video is conditional. If startup qualifies a compatible user-installed FFmpeg with the
fixed MP4/H.264 policy, the same discovery surface also advertises `generate_temporal_video`,
`temporal-video`, and `temporal-video-manifest`. If they are absent, the existing browser and still
evidence surface remains healthy; do not call or invent the video tool.

The first activation can take longer while the release is downloaded and verified. Installer progress
and failures appear on stderr so the MCP protocol on stdout remains valid. Do not run the launcher or
`krometrail mcp` in an ordinary terminal as a health command; it is a stdio protocol server and waits
for an MCP client.

## 3. Standalone CLI and fallback MCP setup

The plugin-managed binary is private to the plugin and intentionally does not add a command to `PATH`.
If the user also wants the standalone CLI, or native plugin MCP loading is unavailable, install the
independent binary only after that request:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
krometrail --version
```

The standalone installer selects the platform asset, verifies `checksums.txt`, executes the candidate's
exact `--version` identity before replacement, and installs atomically. It supports Linux and macOS on
x64 and arm64. Windows uses the published `.exe` asset directly.

After a standalone install, fallback harness configuration can use the direct command:

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

Use each command's `--help` as the authority if the installed harness version differs. Never copy
files from a native plugin cache into manual configuration.

## Troubleshooting boundaries

- **Skill visible, tools absent:** inspect plugin details and MCP status, restart the harness, and
  preserve any launcher error from stderr. `PATH` is not required for native plugin activation.
- **Managed download fails:** check HTTPS access to the GitHub release and asset hosts and confirm the
  plugin version has a matching published release. Never bypass checksum or identity verification.
- **Offline first start:** restore network access for one verified cold activation. Once installed, the
  same plugin version starts offline.
- **Server exits after a verified install:** if a standalone binary exists, run `krometrail --version`
  and `krometrail doctor` outside MCP for diagnostics. Do not replace the native launcher with a shell
  downloader.
- **Browser not found:** install a supported local Chrome/Chromium build or pass an explicit executable
  through `start_browser` as advertised by its tool schema.
- **Temporal-video tool absent:** FFmpeg was not qualified when this MCP server started. If the user
  wants video, make a compatible user-installed FFmpeg discoverable on `PATH` or set
  `KROMETRAIL_FFMPEG_PATH` to its exact executable, then restart the MCP server. Krometrail and its
  plugin do not bundle, download, update, or manage FFmpeg. A later filesystem or `PATH` change does
  not alter the immutable surface of an already-running server.
- **Attach fails:** use an explicitly configured local Chromium debugging endpoint. Do not expose or
  assume a remote endpoint.
- **Tools present but actions require approval:** this plugin intentionally does not auto-allow every
  browser-control tool. Follow operator and harness permission policy.
- **Failed/degraded response includes diagnostics:** use its exact `correlation_id` and `log_path`.
  Search only a narrow correlation-centered excerpt and disclose only sanitized event/error metadata;
  never attach the complete private log.

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

For the sibling marketplace, substitute `nklisch-skills` as the marketplace name. Claude Code can
also auto-update a marketplace and its installed plugins after startup when the operator enables that
marketplace setting; Krometrail does not enable it. Codex updates remain explicit. On the next MCP
activation after a plugin change, the new package verifies and installs its exact matching binary
alongside the prior managed version. Krometrail never polls `latest`, never updates during a warm
start, and never changes an independently installed CLI.

Remove native plugin state with the matching lifecycle:

```bash
claude plugin uninstall krometrail@krometrail --scope user
claude plugin marketplace remove krometrail --scope user

codex plugin remove krometrail@krometrail
codex plugin marketplace remove krometrail
```

Claude owns its plugin data lifecycle. Codex's fallback managed data is stored under
`${XDG_DATA_HOME:-$HOME/.local/share}/krometrail/plugin` and can outlive plugin removal so an offline
rollback remains possible. Independently installed binaries and retained browser evidence are always
separate. Delete managed versions, standalone binaries, or retained evidence only when the user
explicitly requests cleanup and after identifying the exact path and ownership.
