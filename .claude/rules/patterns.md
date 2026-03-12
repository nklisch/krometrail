# Project Patterns

- **Registry + Plugin Interface**: Map-based registry + shared interface + `registerAll*()` at startup — new plugins register, callers never change → [registry-and-plugin.md](.claude/skills/patterns/registry-and-plugin.md)
- **Zod Boundary Validation**: Validate all external inputs (MCP params, RPC, config, DB queries) with Zod schemas at entry points; internal code trusts types → [zod-boundary-validation.md](.claude/skills/patterns/zod-boundary-validation.md)
- **Typed Error Hierarchy**: All errors extend `AgentLensError(message, code)`; domain errors add typed context fields; use `getErrorMessage(err)` for `unknown` → [error-hierarchy.md](.claude/skills/patterns/error-hierarchy.md)
- **Adapter Helpers Module**: All 10 language adapters share `src/adapters/helpers.ts`; use `checkCommand`, `spawnAndWait`, `allocatePort`, `gracefulDispose`, `connectTCP` — never duplicate inline → [adapter-helpers.md](.claude/skills/patterns/adapter-helpers.md)
- **MCP Tool Handler Wrapper**: Wrap every MCP tool with `toolHandler(async (params) => string)` from `src/mcp/tools/utils.ts`; returns `textResponse` or `errorResponse` automatically → [mcp-tool-handler.md](.claude/skills/patterns/mcp-tool-handler.md)
- **Test Factory Functions**: Unit tests define `makeX(overrides?: Partial<T>): T` factories at file top; tests specify only assertion-relevant fields via spread overrides → [test-factory-functions.md](.claude/skills/patterns/test-factory-functions.md)
- **Test skipIf Prerequisites**: Integration/E2E tests compute `SKIP_NO_X = await check().then(ok => !ok)` at module load time and use `describe.skipIf(SKIP_NO_X)` → [test-skipif-prerequisites.md](.claude/skills/patterns/test-skipif-prerequisites.md)
