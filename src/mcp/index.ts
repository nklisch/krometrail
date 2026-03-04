import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { GoAdapter } from "../adapters/go.js";
import { NodeAdapter } from "../adapters/node.js";
import { PythonAdapter } from "../adapters/python.js";
import { registerAdapter } from "../adapters/registry.js";
import { SessionManager } from "../core/session-manager.js";
import { ResourceLimitsSchema } from "../core/types.js";
import { registerTools } from "./tools/index.js";

// Register adapters
registerAdapter(new PythonAdapter());
registerAdapter(new NodeAdapter());
registerAdapter(new GoAdapter());

// Create session manager with default limits
const limits = ResourceLimitsSchema.parse({});
const sessionManager = new SessionManager(limits);

// Create and configure MCP server
const server = new McpServer({
	name: "agent-lens",
	version: "0.1.0",
});

registerTools(server, sessionManager);

// Handle graceful shutdown
process.on("SIGINT", async () => {
	await sessionManager.disposeAll();
	process.exit(0);
});
process.on("SIGTERM", async () => {
	await sessionManager.disposeAll();
	process.exit(0);
});

// Start
const transport = new StdioServerTransport();
await server.connect(transport);
