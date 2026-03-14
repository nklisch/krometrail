# Design: MCP Tool Filtering & `--mcp` CLI Flag

## Overview

Add a `--mcp` flag to the CLI that launches the MCP server inline, and a `--tools` flag that controls which tool groups are exposed. This lets users configure `bugscope --mcp --tools browser` to get only browser observation tools without 18 debug tools cluttering the agent's tool list.

Also supports `BUGSCOPE_TOOLS` env var for the same filtering when using the standalone `src/mcp/index.ts` entry point.

## Implementation Units

### Unit 1: Tool Group Type

**File**: `src/mcp/tool-groups.ts` (new)

```typescript
import { z } from "zod";

export const TOOL_GROUPS = ["debug", "browser"] as const;
export const ToolGroupSchema = z.enum(TOOL_GROUPS);
export type ToolGroup = z.infer<typeof ToolGroupSchema>;

/**
 * Parse a comma-separated tool group string (e.g. "browser" or "debug,browser").
 * Returns all groups if input is undefined/empty.
 * Throws on invalid group names.
 */
export function parseToolGroups(input: string | undefined): Set<ToolGroup> {
	if (!input || input.trim() === "") {
		return new Set(TOOL_GROUPS);
	}
	const groups = input.split(",").map((s) => s.trim());
	const parsed = groups.map((g) => ToolGroupSchema.parse(g));
	return new Set(parsed);
}
```

**Acceptance Criteria**:
- [ ] `parseToolGroups(undefined)` returns `Set(["debug", "browser"])`
- [ ] `parseToolGroups("browser")` returns `Set(["browser"])`
- [ ] `parseToolGroups("debug,browser")` returns `Set(["debug", "browser"])`
- [ ] `parseToolGroups("invalid")` throws a Zod error

---

### Unit 2: Conditional Tool Registration in MCP Entry

**File**: `src/mcp/index.ts`

Change `src/mcp/index.ts` to accept a `Set<ToolGroup>` and conditionally register tools. Extract the server setup into an exported function so both the standalone entry and the CLI `--mcp` flag can call it.

```typescript
import { parseToolGroups, type ToolGroup } from "./tool-groups.js";

export interface McpServerOptions {
	toolGroups?: Set<ToolGroup>;
}

/**
 * Create, configure, and start the MCP server on stdio.
 * Resolves when the transport disconnects.
 */
export async function startMcpServer(options: McpServerOptions = {}): Promise<void> {
	const toolGroups = options.toolGroups ?? parseToolGroups(process.env.BUGSCOPE_TOOLS);

	registerAllAdapters();
	registerAllDetectors();

	const server = new McpServer({ name: "bugscope", version: "0.1.0" });

	let sessionManager: SessionManager | undefined;
	if (toolGroups.has("debug")) {
		sessionManager = createSessionManager();
		registerTools(server, sessionManager);
	}

	let browserDb: BrowserDatabase | undefined;
	if (toolGroups.has("browser")) {
		const browserDataDir = process.env.BUGSCOPE_BROWSER_DATA_DIR ?? resolve(homedir(), ".bugscope", "browser");
		mkdirSync(browserDataDir, { recursive: true });
		browserDb = new BrowserDatabase(resolve(browserDataDir, "index.db"));
		const browserQueryEngine = new QueryEngine(browserDb, browserDataDir);
		registerBrowserTools(server, browserQueryEngine);
	}

	setupGracefulShutdown(() => {
		browserDb?.close();
		return sessionManager?.disposeAll() ?? Promise.resolve();
	});

	const transport = new StdioServerTransport();
	await server.connect(transport);
}

// When run directly (bun run src/mcp/index.ts), start with env-based config
startMcpServer();
```

**Implementation Notes**:
- The top-level `startMcpServer()` call at the bottom ensures backward compatibility when running `bun run src/mcp/index.ts` directly.
- `BUGSCOPE_TOOLS` env var is the fallback when no explicit `toolGroups` option is passed.
- Adapters and detectors are always registered since browser tools may need framework detection.

**Acceptance Criteria**:
- [ ] `startMcpServer()` with no options registers all 29 tools (backward compatible)
- [ ] `startMcpServer({ toolGroups: new Set(["browser"]) })` registers only 11 browser tools
- [ ] `startMcpServer({ toolGroups: new Set(["debug"]) })` registers only 18 debug tools
- [ ] `BUGSCOPE_TOOLS=browser bun run src/mcp/index.ts` registers only browser tools

---

### Unit 3: `--mcp` CLI Flag

**File**: `src/cli/index.ts`

Add `--mcp` and `--tools` as top-level flags on the main command. When `--mcp` is set, import and call `startMcpServer()` instead of running the CLI.

```typescript
const main = defineCommand({
	meta: {
		name: "bugscope",
		version: "0.1.0",
		description: "Runtime debugging viewport for AI coding agents",
	},
	args: {
		mcp: {
			type: "boolean",
			description: "Start as an MCP server on stdio instead of running the CLI",
			default: false,
		},
		tools: {
			type: "string",
			description: "Comma-separated tool groups to expose (debug, browser). Default: all. Only used with --mcp.",
		},
	},
	async run({ args }) {
		if (args.mcp) {
			const { startMcpServer } = await import("../mcp/index.js");
			const { parseToolGroups } = await import("../mcp/tool-groups.js");
			await startMcpServer({ toolGroups: parseToolGroups(args.tools) });
			return;
		}
		// citty shows help by default when no subcommand given
	},
	subCommands: {
		// ... existing subcommands unchanged
	},
});
```

**Implementation Notes**:
- Dynamic `import()` so MCP dependencies aren't loaded for normal CLI usage.
- `--tools` without `--mcp` is silently ignored (not an error).
- citty supports both `--mcp` and `--mcp --tools browser` as top-level args.

**Acceptance Criteria**:
- [ ] `bugscope --mcp` starts the MCP server with all tools
- [ ] `bugscope --mcp --tools browser` starts with only browser tools
- [ ] `bugscope --mcp --tools debug` starts with only debug tools
- [ ] `bugscope --mcp --tools debug,browser` starts with all tools
- [ ] `bugscope --mcp --tools invalid` exits with a Zod validation error
- [ ] `bugscope launch ...` (no `--mcp`) works unchanged
- [ ] `bugscope --tools browser` (no `--mcp`) is silently ignored

---

## Implementation Order

1. **Unit 1** — `tool-groups.ts`: type + parser (no dependencies)
2. **Unit 2** — `src/mcp/index.ts`: extract `startMcpServer()`, conditional registration
3. **Unit 3** — `src/cli/index.ts`: add `--mcp` and `--tools` flags

## Testing

### Unit Tests: `tests/unit/mcp/tool-groups.test.ts`

```typescript
describe("parseToolGroups", () => {
	it("returns all groups for undefined input");
	it("returns all groups for empty string");
	it("parses single group");
	it("parses comma-separated groups");
	it("trims whitespace");
	it("throws on invalid group name");
});
```

### E2E Validation

Manual or scripted validation:

```bash
# All tools (default)
bugscope --mcp  # agent sees 29 tools

# Browser only
bugscope --mcp --tools browser  # agent sees 11 tools

# Debug only
bugscope --mcp --tools debug  # agent sees 18 tools

# Env var fallback
BUGSCOPE_TOOLS=browser bun run src/mcp/index.ts  # 11 tools
```

## Verification Checklist

```bash
bun run test:unit           # parseToolGroups tests pass
bun run lint                # No lint errors
bun run build               # Binary compiles
```
