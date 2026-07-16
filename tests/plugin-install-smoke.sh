#!/usr/bin/env bash
# Opt-in native lifecycle qualification. Uses isolated homes and never invokes a model or Chrome.

set -euo pipefail

if [[ "${KROMETRAIL_PLUGIN_SMOKE:-0}" != "1" ]]; then
  echo "plugin install smoke: skipped (set KROMETRAIL_PLUGIN_SMOKE=1)"
  exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
for command in claude codex jq python3; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "plugin install smoke: $command is required" >&2
    exit 1
  }
done

CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}"
mkdir -p "$CACHE_ROOT"
STATE="$(mktemp -d "$CACHE_ROOT/krometrail-plugin-smoke.XXXXXX")"
cleanup() { rm -rf "$STATE"; }
trap cleanup EXIT

CLAUDE_HOME="$STATE/claude-home"
CODEX_HOME_DIR="$STATE/codex-home"
INSTALL_DIR="$STATE/bin"
mkdir -p "$CLAUDE_HOME" "$CODEX_HOME_DIR" "$INSTALL_DIR"

fail() {
  echo "plugin install smoke failure: $*" >&2
  exit 1
}

# The published installer proves the binary lifecycle independently from plugin state.
release_version="${KROMETRAIL_RELEASE_VERSION:-v1.0.0}"
KROMETRAIL_INSTALL_DIR="$INSTALL_DIR" \
  sh "$ROOT/scripts/install.sh" --version "$release_version" --no-modify-path >/dev/null
[[ "$($INSTALL_DIR/krometrail --version)" == "krometrail ${release_version#v}" ]] || \
  fail "installed binary identity does not match $release_version"
"$INSTALL_DIR/krometrail" --help | grep -Fq 'mcp' || fail "installed binary does not expose mcp"

# Exercise the published stdio server itself: initialize, discover representative tools, and verify
# that exact artifact/frame reads remain MCP resources rather than invented tool calls.
python3 - "$INSTALL_DIR/krometrail" <<'PY'
import json
import select
import subprocess
import sys

process = subprocess.Popen(
    [sys.argv[1], "mcp"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)


def send(message):
    process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
    process.stdin.flush()


def receive():
    ready, _, _ = select.select([process.stdout], [], [], 10)
    if not ready:
        raise RuntimeError("MCP response timed out: " + process.stderr.read(2000))
    return json.loads(process.stdout.readline())


send({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "krometrail-plugin-smoke", "version": "1"},
    },
})
initialized = receive()
assert initialized["result"]["protocolVersion"] == "2025-06-18"
send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
tool_names = {tool["name"] for tool in receive()["result"]["tools"]}
required_tools = {
    "start_browser",
    "observe_live",
    "temporal_debug_bundle",
    "generate_artifacts",
    "fetch_source_frames",
    "query_browser_events",
}
assert required_tools <= tool_names
assert "retrieve_artifact" not in tool_names
assert "retrieve_source_frame" not in tool_names
send({"jsonrpc": "2.0", "id": 3, "method": "resources/templates/list", "params": {}})
templates = {item["name"] for item in receive()["result"]["resourceTemplates"]}
assert templates == {"temporal-artifact", "temporal-source-frame"}
process.stdin.close()
process.wait(timeout=10)
assert process.returncode == 0, process.stderr.read()
PY

export PATH="$INSTALL_DIR:$PATH"

# Claude: register, install, inspect the materialized package, then remove every native layer.
HOME="$CLAUDE_HOME" claude plugin marketplace add "$ROOT" --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin install krometrail@krometrail --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin list --json >"$STATE/claude-list.json"
claude_path="$(jq -r '.[] | select(.id == "krometrail@krometrail") | .installPath' "$STATE/claude-list.json")"
[[ -n "$claude_path" && "$claude_path" != "null" ]] || fail "Claude did not report the installed plugin"
[[ -f "$claude_path/skills/krometrail/SKILL.md" ]] || fail "Claude install omitted the complete skill"
[[ -f "$claude_path/.mcp.json" ]] || fail "Claude install omitted the MCP declaration"
jq -e '.[] | select(.id == "krometrail@krometrail") | .mcpServers.krometrail.command == "krometrail"' \
  "$STATE/claude-list.json" >/dev/null || fail "Claude did not load the direct MCP command"
HOME="$CLAUDE_HOME" claude plugin uninstall krometrail@krometrail --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin marketplace remove krometrail --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin list --json | jq -e 'all(.[]; .id != "krometrail@krometrail")' >/dev/null || \
  fail "Claude plugin remained installed after removal"

# Codex: use its native catalog and explicit component pointers under an isolated CODEX_HOME.
CODEX_HOME="$CODEX_HOME_DIR" codex plugin marketplace add "$ROOT" --json >/dev/null
CODEX_HOME="$CODEX_HOME_DIR" codex plugin add krometrail@krometrail --json >"$STATE/codex-add.json"
codex_path="$(jq -r '.installedPath' "$STATE/codex-add.json")"
[[ -n "$codex_path" && "$codex_path" != "null" ]] || fail "Codex did not report the installed plugin"
[[ -f "$codex_path/skills/krometrail/SKILL.md" ]] || fail "Codex install omitted the complete skill"
[[ -f "$codex_path/.mcp.json" ]] || fail "Codex install omitted the MCP declaration"
CODEX_HOME="$CODEX_HOME_DIR" codex plugin list --json >"$STATE/codex-list.json"
jq -e '.installed[] | select(.pluginId == "krometrail@krometrail" and .enabled == true)' \
  "$STATE/codex-list.json" >/dev/null || fail "Codex did not enable the installed plugin"
CODEX_HOME="$CODEX_HOME_DIR" codex plugin remove krometrail@krometrail >/dev/null
CODEX_HOME="$CODEX_HOME_DIR" codex plugin marketplace remove krometrail >/dev/null
CODEX_HOME="$CODEX_HOME_DIR" codex plugin list --json | \
  jq -e 'all(.installed[]; .pluginId != "krometrail@krometrail")' >/dev/null || \
  fail "Codex plugin remained installed after removal"

printf 'plugin install smoke: ok (%s)\n' "$release_version"
