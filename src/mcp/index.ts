import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import pkg from "../../package.json";
import { registerAllAdapters } from "../adapters/registry.js";
import { QueryEngine } from "../browser/investigation/query-engine.js";
import { BrowserDatabase } from "../browser/storage/database.js";
import { performAutoUpdate } from "../core/auto-update.js";
import { getKrometrailSubdir } from "../core/paths.js";
import { createSessionManager } from "../core/session-manager.js";
import { setupGracefulShutdown } from "../core/shutdown.js";
import { registerAllDetectors } from "../frameworks/index.js";
import { parseToolGroups, type ToolGroup } from "./tool-groups.js";
import { registerBrowserTools } from "./tools/browser.js";
import { registerDebugTools } from "./tools/index.js";

export interface McpServerOptions {
	toolGroups?: Set<ToolGroup>;
}

/**
 * Create, configure, and start the MCP server on stdio.
 * Resolves when the transport disconnects.
 */
export async function startMcpServer(options: McpServerOptions = {}): Promise<void> {
	const toolGroups = options.toolGroups ?? parseToolGroups(process.env.KROMETRAIL_TOOLS);

	registerAllAdapters();
	registerAllDetectors();

	const server = new McpServer({
		name: "krometrail",
		version: pkg.version,
	});

	let sessionManager: ReturnType<typeof createSessionManager> | undefined;
	if (toolGroups.has("debug")) {
		sessionManager = createSessionManager();
		registerDebugTools(server, sessionManager);
	}

	let browserDb: BrowserDatabase | undefined;
	if (toolGroups.has("browser")) {
		const browserDataDir = process.env.KROMETRAIL_BROWSER_DATA_DIR ?? resolve(getKrometrailSubdir("browser"));
		mkdirSync(browserDataDir, { recursive: true });
		browserDb = new BrowserDatabase(resolve(browserDataDir, "index.db"));
		const browserQueryEngine = new QueryEngine(browserDb, browserDataDir);
		registerBrowserTools(server, browserQueryEngine);
	}

	setupGracefulShutdown(() => {
		browserDb?.close();
		return sessionManager?.disposeAll() ?? Promise.resolve();
	});

	performAutoUpdate(); // fire-and-forget

	const transport = new StdioServerTransport();
	await server.connect(transport);
}

// When run directly (bun run src/mcp/index.ts), start with env-based config
startMcpServer();
