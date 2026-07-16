---
id: agent-plugin-distribution
kind: feature
stage: review
tags: [distribution, mcp, documentation]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Distribute Krometrail as a native agent plugin

## Brief

Publish one canonical Krometrail plugin for Claude Code and Codex. The plugin declares the local stdio MCP server, carries one portable skill that helps agents choose and interpret browser and temporal visual evidence, and provides an explicit verified binary bootstrap path when `krometrail` is not installed. The skill follows the user's stated debugging approach instead of imposing a mandatory sequence.

Krometrail's repository remains the plugin source of truth. Native Claude and Codex catalogs expose it directly, while the sibling `../skills` marketplace publishes remote pointers rather than copied manifests or skills. Installation must keep native plugin state, binary availability, MCP connectivity, and agent skill discovery as separately verified facts.

The distribution replaces the obsolete pre-Rust plugin identity: no DAP claims, `chrome_*` namespace, npm fallback, or legacy debugging skills survive in the current package.

## Strategic decisions

- **Guidance style:** one evidence-literacy skill, not a prescribed debugging workflow — agents should match evidence to the user's question and current task.
- **Harnesses:** native Claude Code and Codex manifests/catalogs share the same complete skill and MCP declaration.
- **Binary setup:** explicit agent-invocable use of the checksum-verifying release installer — native plugin installation is not treated as executable installation, and no undocumented post-install hook or MCP-startup downloader is introduced.
- **Publication authority:** canonical assets live in this repository; `../skills` contains only marketplace pointers.
- **Permissions:** the plugin does not silently auto-allow every browser-control tool; operator and harness policy remain authoritative.

## Architectural choice

Use one self-contained cross-vendor plugin directory with sibling native manifests and a shared open-standard skill:

```text
plugin/
  .claude-plugin/plugin.json
  .codex-plugin/plugin.json
  .mcp.json
  settings.json
  skills/krometrail/
    SKILL.md
    agents/openai.yaml
    references/evidence.md
    references/setup.md
```

Claude auto-discovers conventional plugin components; Codex receives explicit `skills` and `mcpServers` pointers. The `.mcp.json` command is the installed `krometrail mcp` binary. Setup guidance invokes the canonical checksum-verifying installer only after the user's request authorizes installation; the MCP command never downloads or executes network content on startup.

This was chosen over separate Claude/Codex skill copies, which would drift, and over a shell launcher that downloads a missing binary during MCP startup, which would hide installation state and execute network code at an unexpected boundary. Three historical skills were also rejected: they describe a removed DAP product and split concepts that current agents usually need together.

## Implementation units

### 1. Canonical native plugin package

**Files:** `plugin/.claude-plugin/plugin.json`, `plugin/.codex-plugin/plugin.json`, `plugin/.mcp.json`, `plugin/settings.json`, `plugin/skills/krometrail/**`

**Story:** `agent-plugin-distribution-canonical-package`

```json
{
  "mcpServers": {
    "krometrail": { "command": "krometrail", "args": ["mcp"] }
  }
}
```

The shared skill starts from user intent, groups current MCP tools by capability, and explains what each evidence class can and cannot establish. It links directly to focused evidence and setup references rather than copying generated parameter schemas. Codex picker metadata stays in `agents/openai.yaml`; portable `SKILL.md` frontmatter contains only `name` and `description`.

**Acceptance criteria:**
- [ ] Both manifests identify Krometrail `1.0.0`, point at the same skill/MCP package where their native schema requires it, and make no DAP or npm-runtime claim.
- [ ] The MCP declaration launches only `krometrail mcp` over stdio and performs no implicit installation.
- [ ] The skill explicitly defers to the user's approach and presents evidence as choices, not a mandatory sequence.
- [ ] The skill accurately covers live observation, structured snapshots, source frames, before/during/after, storyboards, difference maps, region filmstrips, motion history, debug bundles, browser events, gaps, cadence, provenance, and non-diagnostic limits.
- [ ] Binary setup verifies `krometrail --version`, uses the canonical verified installer when authorized, and names restart/reload as a distinct MCP activation step.

### 2. Native marketplace publication

**Files:** `.claude-plugin/marketplace.json`, `.agents/plugins/marketplace.json`, `../skills/.claude-plugin/marketplace.json`, `../skills/.agents/plugins/marketplace.json`

**Story:** `agent-plugin-distribution-marketplace-publication`

Krometrail publishes first-party local-path entries in its own native catalogs. The sibling marketplace keeps only remote `git-subdir` pointers to `nklisch/krometrail`'s `./plugin` directory and current browser/temporal descriptions. Its Codex catalog uses explicit source objects rather than depending on Claude shorthand; existing sibling plugins remain represented.

**Acceptance criteria:**
- [ ] Claude and Codex can register the Krometrail repository directly and discover `krometrail`.
- [ ] Claude and Codex can register `../skills` and discover the same canonical remote plugin without copied skill/manifests.
- [ ] Catalog descriptions and tags remove DAP, language-debugger, and obsolete `chrome_*` claims.
- [ ] Catalog versions and paths agree with plugin metadata and native validators.

### 3. Distribution qualification and operator docs

**Files:** `tests/plugin-static.sh`, `tests/plugin-install-smoke.sh`, `tests/distribution-static.sh`, `README.md`, `docs/guide/development.md`

**Story:** `agent-plugin-distribution-isolated-qualification`

Static checks bind package identity, current descriptions, pointers, MCP command, skill shape, and sibling publisher contract. An opt-in isolated-home smoke test uses installed Claude/Codex CLIs with local marketplace paths, verifies plugin/skill/MCP materialization, then exercises disable/remove or uninstall and marketplace removal without touching the real home. Release verification installs a chosen published version into a temporary user-owned directory and confirms exact binary identity and `mcp` command availability.

**Acceptance criteria:**
- [ ] Ordinary distribution tests catch missing manifests, stale product claims, copied sibling plugin content, and an MCP command that is not the direct binary.
- [ ] Opt-in smoke tests never mutate the operator's actual Claude, Codex, or PATH configuration.
- [ ] A clean v1.0.0 binary install verifies checksums and exact `krometrail 1.0.0` identity.
- [ ] Native plugin install and removal work for both harnesses in isolated homes, with current skill and MCP declarations present.
- [ ] README documents native marketplace install, binary bootstrap, restart/activation, verification, and removal without implying that plugin installation alone installed the executable.

## Implementation order

1. `agent-plugin-distribution-canonical-package`
2. `agent-plugin-distribution-marketplace-publication` depends on the canonical package identity.
3. `agent-plugin-distribution-isolated-qualification` depends on both publication surfaces.

## Simplification

- Replace the stale `plugin/` metadata and empty settings-only shell with one current package.
- Consolidate the old `krometrail-chrome`, `krometrail-debug`, and `krometrail-mcp` concepts into one skill because the Rust product has one browser/temporal MCP surface and no DAP runtime.
- Keep the existing installer as the sole checksum/version authority; do not duplicate download logic in plugin scripts.

## Testing

- Static JSON/shell assertions protect the stable package and publication contracts without depending on harness caches.
- Native CLI smoke tests protect the real marketplace/install/remove seams and are opt-in because installed CLI versions are external environment dependencies.
- Existing Rust MCP tests remain the authority for generated tool schemas and protocol behavior; the skill deliberately does not duplicate them.
- No model invocation or live Chrome launch is required for packaging qualification.

## Risks

- Native plugin schemas are evolving. Qualification uses the installed `claude plugin validate` and real Codex marketplace commands, while static tests keep failures legible.
- Native plugin installation cannot portably install an executable. Setup therefore remains an explicit, verified agent action and MCP activation may require a harness restart.
- A sibling Codex catalog touches unrelated entries. Generate faithful explicit-source entries and verify discovery for the whole catalog rather than silently dropping existing plugins.

## Implementation notes

- **Execution capability:** cohesive inline feature implementation; package, catalogs, docs, and native lifecycle tests shared one identity and were safer under one owner.
- Built the shared Claude/Codex package under `plugin/` with one evidence-literacy skill and direct stdio MCP declaration.
- Published first-party native catalogs and committed the sibling publisher update on `../skills` branch `feat/krometrail-agent-plugin`.
- Added ordinary static distribution contracts and an opt-in real native CLI smoke using isolated homes and the published v1.0.0 binary.
- Corrected one qualification-discovered documentation bug: exact artifact and source-frame reads are MCP resources, not tool calls.
- Corrected stale post-release installer and documentation language and regenerated `docs/public/llms-full.txt`.
- Verification: Claude plugin/marketplace validator; open skill validator; plugin/distribution shell contracts; isolated Claude/Codex install/remove; MCP initialize, 37-tool discovery, and two resource templates; v1.0.0 checksum/identity installer; VitePress build; complete locked Rust fmt/check/test/clippy gate.
- Final egress order after review: push Krometrail, verify sibling remote installs resolve 1.0.0, then publish the sibling branch.
