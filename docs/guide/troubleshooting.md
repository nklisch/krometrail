---
title: Troubleshooting
description: Fix common Krometrail plugin, browser discovery, MCP, permission, and storage problems.
---

# Troubleshooting

Start with the symptom you can observe. Plugin-managed installs and standalone installs have different checks.

## The plugin is installed, but Krometrail tools are missing

1. Restart or reload Claude Code or Codex after installing or updating the plugin.
2. Confirm the Krometrail plugin is enabled and includes both the skill and MCP component.
3. Check your agent's MCP status for a Krometrail connection or startup error.
4. On first activation, allow time for the plugin to download and verify its matching release.

The native plugin uses a private managed binary. It does **not** require a standalone `krometrail` command on `PATH`.

An incompatible retained-evidence schema does not require manual recovery. Retained recordings and
generated artifacts are cache: Krometrail clears only those incompatible cache members and continues
startup. Managed browser profiles, diagnostics, and configuration remain untouched. A storage error
that still prevents startup therefore indicates the cache could not be cleared or initialized; keep
the bounded startup error for diagnosis rather than deleting the entire data directory.

## First activation cannot download the managed binary

The first plugin activation needs HTTPS access to the Krometrail GitHub release and asset hosts. Later starts of the same plugin version use the verified local copy without checking the network.

If the download fails:

- confirm the machine can reach GitHub releases;
- preserve the launcher error shown by your agent's MCP status or stderr;
- verify that the installed plugin version has a matching published Krometrail release;
- do not bypass checksum or executable-identity verification.

## A standalone install cannot find Chrome or Chromium

Run:

```bash
krometrail doctor
```

`doctor` only discovers supported local browsers; it does not launch one. It also never opens recording storage or recording configuration, so you can run it no matter what state your Krometrail data directory is in. The only thing it may write there is the usual best-effort diagnostics log; if that location is unusable, doctor prints a warning and still answers. If it reports `browser_not_found`, install Chrome or a compatible Chromium browser, or have your agent pass an explicit executable through Krometrail's browser-start request.

## The shell cannot find `krometrail`

The standalone installer writes to `~/.local/bin` by default. Restart your shell or add that directory to the current session:

```bash
export PATH="$HOME/.local/bin:$PATH"
krometrail --version
```

This check applies only to a standalone installation. A native plugin-managed binary is private to the plugin and is not added to `PATH`.

## Manual MCP setup cannot start the server

Check the standalone binary first:

```bash
krometrail --version
krometrail doctor
```

Then confirm your MCP client starts this command:

```text
krometrail mcp
```

Do not run `krometrail mcp` as an interactive health check. It is a standard-input/output protocol server and waits for an MCP client; an apparently idle terminal is expected.

See [Manual MCP configuration](mcp-configuration.md) for client commands and JSON configuration.

If only a few tools appear, confirm the client follows every `tools/list` continuation page.
An invalid catalogue cursor means the listing belongs to another process/configuration or is
malformed: discard the cached listing and start again without a cursor. After a plugin update,
start a fresh MCP connection and check the reported server version; installation alone does not
refresh an existing connection.

## The agent asks for permission before controlling the browser

This is expected. The plugin does not bypass your agent harness's tool-approval policy. Review and approve the requested browser lifecycle or control action according to your local policy.

## Krometrail cannot start another browser session

One MCP server owns at most one active browser session. Ask the agent to stop the current controlled browser or detach from the current external browser before starting or attaching another one.

## Attaching to Chrome or Electron fails

Krometrail can attach only to an explicitly enabled **local** Chrome-compatible debugging endpoint.

For Electron, the endpoint exposes Chromium renderer targets. Krometrail does not inspect or control Electron's Node main process. Confirm the application enabled local remote debugging and that the agent is using the correct endpoint.

Do not expose a debugging endpoint on a public network interface.

## Evidence has gaps or an interval is unavailable

Chrome can pause or reduce screencast delivery for hidden tabs, and local load or bounded ingestion can create known capture gaps. Krometrail reports those gaps rather than treating the interval as continuously observed.

If older evidence was evicted, reproduce the interaction and ask the agent to pin the new interval while investigating it. If pinned evidence consumes the disk budget, capture pauses instead of deleting protected data; unpin ranges you no longer need.

## Windows installation

The one-line installer supports Linux and macOS on x64 and arm64. Windows is a best-effort direct-download `.exe` from [GitHub Releases](https://github.com/nklisch/krometrail/releases/latest), not an installer-supported or supported development environment. Verify the matching checksum before use.

## Report a problem

Open an issue on [GitHub](https://github.com/nklisch/krometrail/issues) with:

- `krometrail --version` for a standalone install, or the plugin version for a managed install;
- operating system and browser version;
- `krometrail doctor` output when browser discovery is involved;
- the MCP startup error or relevant stderr text;
- the smallest reproduction you can share safely.

Captured browser content stays local. Share screenshots, frames, URLs, or page data only when you intend to include them in the report.
