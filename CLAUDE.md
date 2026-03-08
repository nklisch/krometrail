# Agent Lens

MCP server + CLI that gives AI agents runtime debugging via the Debug Adapter Protocol.

## Project Structure

```
src/
  cli/          CLI entry point + commands (citty)
  mcp/          MCP server entry + tool handlers (@modelcontextprotocol/sdk)
  core/         Session manager, viewport renderer, DAP client, value renderer
  adapters/     Language-specific debugger adapters
  index.ts      Library exports
tests/
  unit/           Pure logic tests (mocks OK)
  integration/    Real debugger tests (no mocks)
  e2e/            Full MCP/CLI-to-viewport tests (no mocks)
  fixtures/       Real programs used as debug targets
  agent-harness/  Scenario-based test suite for evaluating agent debugging ability
    scenarios/    Buggy programs at 5 difficulty levels, per-language suites
docs/             Design docs (VISION, ARCH, UX, SPEC, INTERFACE, TESTING)
  designs/        Phase design docs — named phase-N-short-description.md
```

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
