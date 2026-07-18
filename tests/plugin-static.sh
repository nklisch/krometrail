#!/usr/bin/env bash
# Static package contracts for the native Claude Code and Codex distribution.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN="$ROOT/plugin"
CLAUDE_MANIFEST="$PLUGIN/.claude-plugin/plugin.json"
CODEX_MANIFEST="$PLUGIN/.codex-plugin/plugin.json"
CLAUDE_MCP="$PLUGIN/.mcp.json"
CODEX_MCP="$PLUGIN/.mcp.codex.json"
PLUGIN_VERSION="$PLUGIN/version"
LAUNCHER="$PLUGIN/bin/krometrail"
MANAGED_INSTALLER="$PLUGIN/scripts/install-managed.sh"
SKILL="$PLUGIN/skills/krometrail/SKILL.md"
EVIDENCE="$PLUGIN/skills/krometrail/references/evidence.md"
SETUP="$PLUGIN/skills/krometrail/references/setup.md"
OPENAI="$PLUGIN/skills/krometrail/agents/openai.yaml"
REPORT_SKILL="$PLUGIN/skills/report-krometrail-issue/SKILL.md"
REPORT_OPENAI="$PLUGIN/skills/report-krometrail-issue/agents/openai.yaml"
CLAUDE_MARKETPLACE="$ROOT/.claude-plugin/marketplace.json"
CODEX_MARKETPLACE="$ROOT/.agents/plugins/marketplace.json"

fail() {
  echo "plugin contract failure: $*" >&2
  exit 1
}

require_text() {
  local file="$1"
  local text="$2"
  grep -Fq -- "$text" "$file" || fail "${file#"$ROOT/"} is missing: $text"
}

for file in \
  "$CLAUDE_MANIFEST" "$CODEX_MANIFEST" "$CLAUDE_MCP" "$CODEX_MCP" "$PLUGIN_VERSION" \
  "$LAUNCHER" "$MANAGED_INSTALLER" "$SKILL" "$EVIDENCE" "$SETUP" "$OPENAI" \
  "$REPORT_SKILL" "$REPORT_OPENAI" \
  "$CLAUDE_MARKETPLACE" "$CODEX_MARKETPLACE"; do
  [[ -f "$file" ]] || fail "missing ${file#"$ROOT/"}"
done

command -v jq >/dev/null 2>&1 || fail "jq is required"
jq empty "$CLAUDE_MANIFEST" "$CODEX_MANIFEST" "$CLAUDE_MCP" "$CODEX_MCP" "$CLAUDE_MARKETPLACE" "$CODEX_MARKETPLACE"

cargo_version="$(awk '
  /^\[package\][[:space:]]*$/ { in_package=1; next }
  /^\[/ { in_package=0 }
  in_package && /^[[:space:]]*version[[:space:]]*=/ {
    gsub(/[[:space:]"]/, "", $0); sub(/^version=/, "", $0); print; exit
  }
' "$ROOT/Cargo.toml")"
[[ -n "$cargo_version" ]] || fail "could not read Cargo package version"

for manifest in "$CLAUDE_MANIFEST" "$CODEX_MANIFEST"; do
  jq -e --arg version "$cargo_version" '
    .name == "krometrail" and
    .version == $version and
    .repository == "https://github.com/nklisch/krometrail" and
    .license == "MIT"
  ' "$manifest" >/dev/null || fail "${manifest#"$ROOT/"} identity does not match Cargo"
done

jq -e '
  .mcpServers == "./.mcp.json"
' "$CLAUDE_MANIFEST" >/dev/null || fail "Claude MCP component pointer is incomplete"

jq -e '
  .skills == "./skills/" and
  .mcpServers == "./.mcp.codex.json" and
  .interface.displayName == "Krometrail"
' "$CODEX_MANIFEST" >/dev/null || fail "Codex component pointers are incomplete"

jq -e '
  keys == ["mcpServers"] and
  (.mcpServers | keys) == ["krometrail"] and
  .mcpServers.krometrail == {
    "command":"${CLAUDE_PLUGIN_ROOT}/bin/krometrail",
    "args":["mcp"],
    "env":{"KROMETRAIL_MANAGED_ROOT":"${CLAUDE_PLUGIN_DATA}"}
  }
' "$CLAUDE_MCP" >/dev/null || fail "Claude MCP must launch the package-owned managed binary"

jq -e '
  keys == ["krometrail"] and
  .krometrail == {"command":"sh","args":["bin/krometrail","mcp"],"cwd":"."}
' "$CODEX_MCP" >/dev/null || fail "Codex MCP must launch relative to the installed plugin root"

[[ "$(cat "$PLUGIN_VERSION")" == "$cargo_version" ]] || fail "plugin version marker does not match Cargo"
[[ -x "$LAUNCHER" && -x "$MANAGED_INSTALLER" ]] || fail "plugin bootstrap scripts must be executable"
require_text "$LAUNCHER" 'exec "$MANAGED_BINARY" "$@"'
require_text "$LAUNCHER" 'verify-existing'
require_text "$MANAGED_INSTALLER" 'verify_existing_destination'
require_text "$MANAGED_INSTALLER" 'checksum verification failed'
require_text "$MANAGED_INSTALLER" 'release-assets.githubusercontent.com'

jq -e '.permissions.allow == []' "$PLUGIN/settings.json" >/dev/null || \
  fail "plugin must not silently auto-allow browser-control tools"

jq -e --arg version "$cargo_version" '
  .name == "krometrail" and
  .plugins == [{
    "name":"krometrail",
    "source":"./plugin",
    "description":"Control a local Chromium browser and inspect transient behavior with source-linked temporal visual evidence.",
    "category":"development",
    "version":$version
  }]
' "$CLAUDE_MARKETPLACE" >/dev/null || fail "Claude marketplace does not publish the canonical plugin"

jq -e --arg version "$cargo_version" '
  .name == "krometrail" and
  (.plugins | length) == 1 and
  .plugins[0].name == "krometrail" and
  .plugins[0].source == {"source":"local","path":"./plugin"} and
  .plugins[0].version == $version and
  .plugins[0].policy.installation == "AVAILABLE" and
  .plugins[0].policy.authentication == "ON_INSTALL"
' "$CODEX_MARKETPLACE" >/dev/null || fail "Codex marketplace does not publish the canonical plugin"

frontmatter="$(awk 'NR == 1 && $0 == "---" { active=1; next } active && $0 == "---" { exit } active { print }' "$SKILL")"
[[ "$frontmatter" == *"name: krometrail"* ]] || fail "skill name is missing"
[[ "$frontmatter" == *"description:"* ]] || fail "skill description is missing"
if grep -Eq '^(license|compatibility|metadata|allowed-tools|user-invocable|model|effort|argument-hint):' <<<"$frontmatter"; then
  fail "portable skill frontmatter contains a harness-specific field"
fi
require_text "$OPENAI" 'allow_implicit_invocation: true'
require_text "$OPENAI" 'Use $krometrail'
report_frontmatter="$(awk 'NR == 1 && $0 == "---" { active=1; next } active && $0 == "---" { exit } active { print }' "$REPORT_SKILL")"
[[ "$report_frontmatter" == *"name: report-krometrail-issue"* ]] || fail "reporting skill name is missing"
require_text "$REPORT_OPENAI" 'Use $report-krometrail-issue'
for term in \
  'nklisch/krometrail' \
  'gh auth status' \
  'gh issue list' \
  'explicit confirmation' \
  'never include a whole log'; do
  grep -Fiq -- "$term" "$REPORT_SKILL" || fail "reporting skill is missing: $term"
done

for term in \
  'Follow the user' \
  'observe_live' \
  'temporal_debug_bundle' \
  'generate_region_filmstrip' \
  'query_browser_events' \
  'capture gap' \
  'manifest_uri' \
  'provenance'; do
  require_text "$SKILL" "$term"
done
for term in \
  'use `generate_temporal_video` only when it is advertised' \
  '`real_time`' \
  '`model_optimized`' \
  'do not upload either'; do
  require_text "$SKILL" "$term"
done
for term in \
  'full document-body text' \
  'use a locator for exact element text' \
  'match_mode: "contains"'; do
  require_text "$SKILL" "$term"
done
for term in \
  'Before/during/after' \
  'Storyboard' \
  'Difference map' \
  'Region filmstrip' \
  'Motion history' \
  'Source frames' \
  'does not diagnose'; do
  grep -Fiq -- "$term" "$EVIDENCE" "$SKILL" || fail "evidence guidance is missing: $term"
done
require_text "$SETUP" 'https://krometrail.dev/install.sh'
require_text "$SETUP" 'Plugin installation, managed-binary activation, MCP connection, and tool discovery are separately'
require_text "$SETUP" 'does not depend on `krometrail` being on `PATH`'
require_text "$SETUP" 'never polls `latest`'
require_text "$SETUP" 'temporal-artifact-manifest'
require_text "$SETUP" '`generate_temporal_video`'
require_text "$SETUP" '`KROMETRAIL_FFMPEG_PATH`'
require_text "$SETUP" 'then restart the MCP server'
require_text "$SETUP" 'do not bundle, download, update, or manage FFmpeg'
require_text "$SETUP" 'claude plugin install krometrail@krometrail'
require_text "$SETUP" 'codex plugin add krometrail@krometrail'
for term in \
  'Prefer storyboards, difference maps, and targeted source reads first' \
  'presentation segments' \
  'does not establish that a particular host, provider, or model'; do
  require_text "$EVIDENCE" "$term"
done
if find "$PLUGIN" -type f -iname '*ffmpeg*' | grep -q .; then
  fail "plugin must not ship an FFmpeg-named asset"
fi
if grep -Eiq 'ffmpeg|libx264' "$LAUNCHER" "$MANAGED_INSTALLER"; then
  fail "plugin bootstrap must not acquire or manage FFmpeg"
fi
if grep -Fq -- '`retrieve_artifact`' "$SKILL" || grep -Fq -- '`retrieve_source_frame`' "$SKILL"; then
  fail "resource-only evidence reads must not be presented as tools"
fi

if grep -ERiq --exclude='plugin-static.sh' \
  'debug 10 languages|via DAP|chrome_[a-z_]+|npx[[:space:]]+krometrail' \
  "$PLUGIN" "$CLAUDE_MARKETPLACE" "$CODEX_MARKETPLACE"; then
  fail "plugin contains a removed TypeScript/DAP-era contract"
fi

# The sibling marketplace is a publisher, never a copied package authority.
SKILLS_REPO="${KROMETRAIL_SKILLS_REPO:-$ROOT/../skills}"
if [[ -n "${KROMETRAIL_SKILLS_REPO:-}" || ( -f "$SKILLS_REPO/.claude-plugin/marketplace.json" && -f "$SKILLS_REPO/.agents/plugins/marketplace.json" ) ]]; then
  sibling_claude="$SKILLS_REPO/.claude-plugin/marketplace.json"
  sibling_codex="$SKILLS_REPO/.agents/plugins/marketplace.json"
  [[ -f "$sibling_claude" && -f "$sibling_codex" ]] || fail "sibling native catalogs are incomplete"
  jq -e --arg version "$cargo_version" '
    .plugins[] | select(.name == "krometrail") |
    .source == {"source":"git-subdir","url":"https://github.com/nklisch/krometrail","path":"./plugin"} and
    .version == $version
  ' "$sibling_claude" >/dev/null || fail "sibling Claude catalog has a stale Krometrail pointer"
  jq -e --arg version "$cargo_version" '
    .plugins[] | select(.name == "krometrail") |
    .source == {"source":"git-subdir","url":"https://github.com/nklisch/krometrail","path":"./plugin"} and
    .version == $version
  ' "$sibling_codex" >/dev/null || fail "sibling Codex catalog has a stale Krometrail pointer"
  [[ ! -d "$SKILLS_REPO/plugins/krometrail" ]] || fail "sibling must not copy the canonical plugin"
fi

if command -v claude >/dev/null 2>&1; then
  claude plugin validate "$CLAUDE_MARKETPLACE" >/dev/null
  claude plugin validate "$PLUGIN" >/dev/null
fi

printf 'plugin distribution contracts: ok\n'
