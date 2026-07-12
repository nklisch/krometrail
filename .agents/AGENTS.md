# Krometrail

> **Direction reset:** The authoritative foundation is `docs/VISION.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/VISUAL-EVIDENCE.md`, and `docs/EVALUATION.md`. The TypeScript/DAP instructions below describe the `v0.2.20` reference implementation while the Rust temporal browser recorder replaces it; they do not define the intended architecture.

MCP server + CLI that gives AI agents runtime debugging via the Debug Adapter Protocol.

## Project Structure

```
src/
  cli/          CLI entry point + commands (citty)
  mcp/          MCP server entry + tool handlers (@modelcontextprotocol/sdk)
  core/         Session manager, viewport renderer, DAP client, value renderer
  adapters/     Language-specific debugger adapters
  browser/      Browser CDP session, lens pipeline, investigation tools
  daemon/       Background daemon for browser session persistence
  frameworks/   Framework detection (pytest, jest, Spring, etc.)
  index.ts      Library exports
tests/
  unit/           Pure logic tests (mocks OK)
  integration/    Real debugger tests (no mocks)
  e2e/            Full MCP/CLI-to-viewport tests (no mocks)
  fixtures/       Real programs used as debug targets
  agent-harness/  Scenario-based test suite for evaluating agent debugging ability
    scenarios/    Buggy programs at 5 difficulty levels, per-language suites
docs/             Current foundation plus legacy v0.2.20 documentation
  agents.md       Navigation guide — which docs are authoritative
  legacy/         Outdated v0.2.20 docs — do not use for current direction
  .generated/     Auto-generated v0.2.20 reference docs (do not edit directly)
```

## Documentation Rules

- Treat `docs/VISION.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/VISUAL-EVIDENCE.md`, and `docs/EVALUATION.md` as authoritative.
- Historical design documents are preserved at Git tag `v0.2.20`; they do not define current work.
- **Do not read `docs/legacy/`** for current direction. Those docs describe the replaced product.
- **Do not edit `docs/.generated/`** files directly. They belong to the `v0.2.20` reference implementation.
- See `docs/agents.md` for full navigation guidance.

## Commands

```bash
bun install              # Install deps
bun run dev              # Run CLI in dev
bun run mcp              # Run MCP server
bun run build            # Compile binary
bun run test             # All tests
bun run test:unit        # Unit tests only (fast)
bun run test:integration # Integration tests (needs debuggers)
bun run test:e2e         # E2E tests (needs debuggers)
bun run lint             # Biome check
bun run lint:fix         # Biome auto-fix
```

## Releasing

Use the bump script to create and push a release:

```bash
bun scripts/bump-version.ts minor   # or patch | major | x.y.z
```

This updates `package.json`, commits, tags, and pushes. The `v*` tag triggers CI which builds platform binaries, creates a GitHub Release, and publishes to npm.

After the release, update your local binary:

```bash
bash scripts/dev-install.sh
```

The user-facing installer lives at `scripts/install.sh` and is served via the docs site at `https://krometrail.dev/install.sh`.

## Stack

- **Runtime:** Bun
- **Validation:** Zod 4 on all boundaries (MCP inputs, adapter configs, viewport config)
- **CLI:** citty (unjs)
- **Logging:** consola
- **DAP:** @vscode/debugprotocol types + custom minimal client
- **Testing:** vitest — e2e/integration tests use real debuggers, no mocks
- **Lint/Format:** Biome

## Conventions

- Tabs for indentation, 200 char line width (Biome config)
- Validate all external inputs with Zod schemas at boundaries
- Adapters implement the `DebugAdapter` interface in `src/adapters/base.ts`
- Viewport output is the contract — if the format changes, e2e tests should break
- Do not include "Co-Authored-By" or any AI signature lines in commit messages

## Bun adapter (unsupported)

`src/adapters/bun.ts` exists but is **not registered** in `registerAllAdapters()`. Bun 1.3.x uses WebKit JSC inspector protocol (not V8 CDP) — `Debugger.paused` events never fire regardless of how breakpoints are set. js-debug is V8/CDP-only and cannot bridge to WebKit protocol. The adapter code is kept for reference. Re-enable when Bun's CDP supports programmatic pause (`Debugger.paused`), or rewrite using `@rttnd/bun-inspector-protocol` (WebKit protocol wrapper). Relevant Bun issues: #4842, #9290, #13994.

<!-- agile-workflow:start -->
## Agile-Workflow Substrate

Work tracked in `.work/` as markdown items with YAML frontmatter (`kind, stage, tags, parent, depends_on, release_binding, research_refs, research_origin`; `[research]` items also carry the commissioning `research_dials` block). Layout: `.work/active/{epics,features,stories}/`, `.work/backlog/`, `.work/releases/<version>/`, `.work/archive/`. The `.work/` ↔ `.research/` handoff follows the agentic-research plugin contract.

**Primary query tool:** `.work/bin/work-view` filters by stage, tag, kind, parent, and dependency. Common patterns:
- `work-view --ready` — items ready to work (deps satisfied)
- `work-view --stage review` — items awaiting an agent review pass (`/agile-workflow:review`)
- `work-view --parent <id>` / `--blocking <id>` — hierarchy / sequencing
- `work-view --scope all` — include terminal tiers: `releases/` (one summary doc per version) and `archive/` (bodyless ref stubs). Full bodies live in git history. By default work-view shows only active + backlog; `--release` / `--gate` auto-widen to all tiers.
- `work-view --help` for the full flag set

Foundation docs in `docs/` describe the system's current state or intended future state, never the past; git history is the audit trail. Item files are the durable state: update the body with implementation discoveries, review findings, blockers, and decisions instead of relying on chat history.

Reusable code patterns live in `.agents/skills/patterns/` (load the `patterns` skill for detail). Project agent rules live in `.agents/rules/*.md` (plugin-managed rules in `.agents/rules/agile-workflow.md`); do not maintain `.claude/rules/*.md` as a source of truth.

**Before designing, implementing, or reviewing, read `.agents/rules/*.md`** — the project's force-loaded agent rules (tag semantics, test integrity, review policy). The agile-workflow hook auto-loads these at session start and after compaction; read them directly when working without the hook. Do not rely on UserPromptSubmit for rules or queue snapshots; query `work-view` when queue state is needed.

Project-specific refactor style conventions belong in this file under `## Refactor Style Conventions`. Detailed refactor convention references belong in `.agents/skills/refactor-conventions/` and extend `refactor-design`'s defaults; they do not replace the built-in scan and they do not create standalone plan docs.

<!-- agile-workflow:end -->
