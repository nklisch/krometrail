import { KrometrailError, SessionLimitError, SessionNotFoundError, SessionStateError } from "../../core/errors.js";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import { getDaemonSocketPath, RPC_ADAPTER_ERROR, RPC_LAUNCH_ERROR, RPC_SESSION_LIMIT_ERROR, RPC_SESSION_NOT_FOUND, RPC_SESSION_STATE_ERROR } from "../../daemon/protocol.js";
import { exitCodeFromError } from "../exit-codes.js";
import type { OutputMode } from "../format.js";
import { formatError, resolveOutputMode } from "../format.js";

// --- Shared CLI Args ---

export const globalArgs = {
	json: {
		type: "boolean" as const,
		description: "Output as JSON envelope { ok, data } or { ok: false, error }",
		default: false,
	},
	quiet: {
		type: "boolean" as const,
		description: "Minimal output (viewport only, no banners or hints)",
		default: false,
	},
	session: {
		type: "string" as const,
		description: "Target a specific session (required when multiple active)",
		alias: "s",
	},
};

/**
 * Create a DaemonClient, ensuring daemon is running first.
 */
export async function getClient(timeoutMs = 60_000): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: timeoutMs });
}

/**
 * Resolve session ID. If --session is provided, use it.
 * Otherwise call daemon.sessions to auto-resolve if exactly one session exists.
 */
export async function resolveSessionId(client: DaemonClient, explicitSession?: string): Promise<string> {
	if (explicitSession) {
		return explicitSession;
	}

	const sessions = await client.call<Array<{ id: string; status: string; language: string; actionCount: number }>>("daemon.sessions");

	if (sessions.length === 0) {
		throw new Error('No active sessions. Launch one with: krometrail debug launch "<command>"');
	}

	if (sessions.length === 1) {
		return sessions[0].id;
	}

	const sessionList = sessions.map((s) => `  ${s.id} (${s.language}, ${s.status})`).join("\n");
	throw new Error(`Multiple active sessions. Use --session to specify one:\n${sessionList}`);
}

/**
 * Attempt to reconstruct a typed KrometrailError from a daemon RPC error.
 * The daemon sends JSON-RPC errors with numeric codes. The DaemonClient wraps
 * them as generic KrometrailError(message, String(code)).
 *
 * This function maps known RPC codes to their proper error subclasses so
 * exitCodeFromError() can classify them accurately.
 */
export function classifyError(err: unknown): unknown {
	if (!(err instanceof KrometrailError)) {
		return err;
	}

	const numericCode = Number(err.code);
	if (Number.isNaN(numericCode)) {
		// Already a properly-coded KrometrailError (e.g. SessionNotFoundError)
		return err;
	}

	// Map RPC numeric codes → typed error subclasses
	switch (numericCode) {
		case RPC_SESSION_NOT_FOUND:
			// Extract session id from message if possible
			return new SessionNotFoundError(err.message);
		case RPC_SESSION_STATE_ERROR:
			return new SessionStateError("", "unknown", []);
		case RPC_SESSION_LIMIT_ERROR:
			return new SessionLimitError("sessions", 0, 0);
		case RPC_ADAPTER_ERROR:
			return new KrometrailError(err.message, "ADAPTER_ERROR");
		case RPC_LAUNCH_ERROR:
			return new KrometrailError(err.message, "LAUNCH_FAILED");
		default:
			return new KrometrailError(err.message, "RPC_ERROR");
	}
}

/**
 * Wrap a CLI command with standard mode resolution, client lifecycle,
 * session resolution, error handling, and semantic exit codes.
 *
 * For commands that don't need a session ID upfront (e.g. launch), pass
 * `{ needsSession: false }` and the handler receives an empty string as sessionId.
 */
export async function runCommand(
	args: { json?: boolean; quiet?: boolean; session?: string },
	handler: (client: DaemonClient, sessionId: string, mode: OutputMode) => Promise<void>,
	opts?: { needsSession?: false },
): Promise<void> {
	const mode = resolveOutputMode(args) as OutputMode;
	const client = await getClient();
	try {
		const sessionId = opts?.needsSession === false ? "" : await resolveSessionId(client, args.session);
		await handler(client, sessionId, mode);
	} catch (err) {
		const classified = classifyError(err);
		process.stderr.write(`${formatError(classified, mode)}\n`);
		process.exit(exitCodeFromError(classified));
	} finally {
		client.dispose();
	}
}

export type { OutputMode };
