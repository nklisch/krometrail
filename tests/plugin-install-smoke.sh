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
CODEX_DATA="$STATE/codex-data"
mkdir -p "$CLAUDE_HOME" "$CODEX_HOME_DIR" "$CODEX_DATA"

fail() {
  echo "plugin install smoke failure: $*" >&2
  exit 1
}

release_version="v$(cat "$ROOT/plugin/version")"
if [[ -n "${KROMETRAIL_RELEASE_VERSION:-}" && "$KROMETRAIL_RELEASE_VERSION" != "$release_version" ]]; then
  fail "requested release $KROMETRAIL_RELEASE_VERSION does not match plugin $release_version"
fi

cat >"$STATE/probe_mcp.py" <<'PY'
import json
import os
import select
import subprocess
import sys

cwd = sys.argv[1]
command = sys.argv[2:]
process = subprocess.Popen(
    command,
    cwd=cwd,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
    env=os.environ.copy(),
)


def send(message):
    process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
    process.stdin.flush()


def receive():
    ready, _, _ = select.select([process.stdout], [], [], 30)
    if not ready:
        raise RuntimeError("MCP response timed out: " + process.stderr.read(4000))
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
assert templates == {
    "managed-download",
    "temporal-artifact",
    "temporal-artifact-manifest",
    "temporal-source-frame",
}
process.stdin.close()
process.wait(timeout=10)
assert process.returncode == 0, process.stderr.read()
PY

# Claude: native plugin loading resolves package/data placeholders and starts the launcher automatically.
HOME="$CLAUDE_HOME" claude plugin marketplace add "$ROOT" --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin install krometrail@krometrail --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin list --json >"$STATE/claude-list.json"
claude_path="$(jq -r '.[] | select(.id == "krometrail@krometrail") | .installPath' "$STATE/claude-list.json")"
[[ -n "$claude_path" && "$claude_path" != "null" ]] || fail "Claude did not report the installed plugin"
[[ -f "$claude_path/skills/krometrail/SKILL.md" ]] || fail "Claude install omitted the complete skill"
[[ -f "$claude_path/skills/report-krometrail-issue/SKILL.md" ]] || fail "Claude install omitted the reporting skill"
[[ -x "$claude_path/bin/krometrail" && -x "$claude_path/scripts/install-managed.sh" ]] || fail "Claude install omitted executable bootstrap files"
jq -e '.[] | select(.id == "krometrail@krometrail") |
  .mcpServers.krometrail.command == "${CLAUDE_PLUGIN_ROOT}/bin/krometrail" and
  .mcpServers.krometrail.env.KROMETRAIL_MANAGED_ROOT == "${CLAUDE_PLUGIN_DATA}"' \
  "$STATE/claude-list.json" >/dev/null || fail "Claude did not load the managed MCP declaration"
HOME="$CLAUDE_HOME" MCP_TIMEOUT=60000 claude mcp list >"$STATE/claude-mcp.txt" 2>&1
(grep -F 'plugin:krometrail:krometrail:' "$STATE/claude-mcp.txt" | grep -Fq 'Connected') || fail "Claude did not start the managed MCP server"
claude_data="$CLAUDE_HOME/.claude/plugins/data/krometrail-krometrail"
claude_binary="$claude_data/versions/${release_version#v}/krometrail"
[[ -x "$claude_binary" ]] || fail "Claude activation did not install the managed binary"
[[ "$($claude_binary --version)" == "krometrail ${release_version#v}" ]] || fail "Claude managed binary has the wrong identity"
HOME="$CLAUDE_HOME" KROMETRAIL_MANAGED_ROOT="$claude_data" \
  python3 "$STATE/probe_mcp.py" "$claude_path" "$claude_path/bin/krometrail" mcp
HOME="$CLAUDE_HOME" claude plugin details krometrail@krometrail >"$STATE/claude-details.txt"
grep -Eq 'Skills \(2\).*krometrail' "$STATE/claude-details.txt" || fail "Claude did not discover both Krometrail skills"
HOME="$CLAUDE_HOME" claude plugin uninstall krometrail@krometrail --scope user >/dev/null
HOME="$CLAUDE_HOME" claude plugin marketplace remove krometrail --scope user >/dev/null
[[ ! -e "$claude_data" ]] || fail "Claude uninstall did not remove managed plugin data"

# Codex: native loading resolves cwd against the installed plugin; invoking that declaration bootstraps MCP.
CODEX_HOME="$CODEX_HOME_DIR" codex plugin marketplace add "$ROOT" --json >/dev/null
CODEX_HOME="$CODEX_HOME_DIR" codex plugin add krometrail@krometrail --json >"$STATE/codex-add.json"
codex_path="$(jq -r '.installedPath' "$STATE/codex-add.json")"
[[ -n "$codex_path" && "$codex_path" != "null" ]] || fail "Codex did not report the installed plugin"
[[ -f "$codex_path/skills/krometrail/SKILL.md" ]] || fail "Codex install omitted the complete skill"
[[ -f "$codex_path/skills/report-krometrail-issue/SKILL.md" ]] || fail "Codex install omitted the reporting skill"
[[ -x "$codex_path/bin/krometrail" && -x "$codex_path/scripts/install-managed.sh" ]] || fail "Codex install omitted executable bootstrap files"
CODEX_HOME="$CODEX_HOME_DIR" codex mcp list --json >"$STATE/codex-mcp.json"
jq -e --arg cwd "$codex_path/." '.[] | select(.name == "krometrail") |
  .transport.command == "sh" and
  .transport.args == ["bin/krometrail", "mcp"] and
  .transport.cwd == $cwd' "$STATE/codex-mcp.json" >/dev/null || fail "Codex did not resolve the plugin-relative MCP declaration"
HOME="$CODEX_HOME_DIR" XDG_DATA_HOME="$CODEX_DATA" CODEX_HOME="$CODEX_HOME_DIR" \
  python3 "$STATE/probe_mcp.py" "$codex_path" sh bin/krometrail mcp
codex_binary="$CODEX_DATA/krometrail/plugin/versions/${release_version#v}/krometrail"
[[ -x "$codex_binary" ]] || fail "Codex activation did not install the managed binary"
[[ "$($codex_binary --version)" == "krometrail ${release_version#v}" ]] || fail "Codex managed binary has the wrong identity"
CODEX_HOME="$CODEX_HOME_DIR" codex debug prompt-input 'probe skills' >"$STATE/codex-prompt.json"
grep -Fq 'krometrail:krometrail' "$STATE/codex-prompt.json" || fail "Codex did not expose the Krometrail skill to the model"
grep -Fq 'krometrail:report-krometrail-issue' "$STATE/codex-prompt.json" || fail "Codex did not expose the reporting skill to the model"
CODEX_HOME="$CODEX_HOME_DIR" codex plugin remove krometrail@krometrail >/dev/null
CODEX_HOME="$CODEX_HOME_DIR" codex plugin marketplace remove krometrail >/dev/null
[[ -x "$codex_binary" ]] || fail "Codex removal unexpectedly deleted fallback XDG managed data"

printf 'plugin install smoke: ok (%s, managed bootstrap)\n' "$release_version"
