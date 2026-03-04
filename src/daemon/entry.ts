/**
 * Daemon entry point — spawned as a detached background process by the CLI.
 * This file is the target of `bun run src/daemon/entry.ts`.
 *
 * Registers adapters, creates SessionManager, starts DaemonServer.
 * Detaches stdio so the parent CLI process can exit.
 */

import { GoAdapter } from "../adapters/go.js";
import { NodeAdapter } from "../adapters/node.js";
import { PythonAdapter } from "../adapters/python.js";
import { registerAdapter } from "../adapters/registry.js";
import { SessionManager } from "../core/session-manager.js";
import { ResourceLimitsSchema } from "../core/types.js";
import { getDaemonPidPath, getDaemonSocketPath } from "./protocol.js";
import { DaemonServer } from "./server.js";

// Register adapters (same as mcp/index.ts)
registerAdapter(new PythonAdapter());
registerAdapter(new NodeAdapter());
registerAdapter(new GoAdapter());

const limits = ResourceLimitsSchema.parse({});
const sessionManager = new SessionManager(limits);

const server = new DaemonServer(sessionManager, {
	socketPath: getDaemonSocketPath(),
	pidPath: getDaemonPidPath(),
	idleTimeoutMs: 60_000,
});

await server.start();

process.on("SIGINT", async () => {
	await server.shutdown();
	process.exit(0);
});

process.on("SIGTERM", async () => {
	await server.shutdown();
	process.exit(0);
});
