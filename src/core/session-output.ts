import type { DebugProtocol } from "@vscode/debugprotocol";
import type { DAPClient } from "./dap-client.js";
import { formatSessionLogDetailed, formatSessionLogSummary } from "./session-logger.js";
import type { DebugSession } from "./session-manager.js";

/**
 * Add watch expressions to the session.
 */
export function addSessionWatchExpressions(session: DebugSession, expressions: string[], logAction: (session: DebugSession, tool: string, summary: string) => void): string[] {
	for (const expr of expressions) {
		if (!session.watchExpressions.includes(expr)) {
			session.watchExpressions.push(expr);
		}
	}
	logAction(session, "debug_watch", `Added ${expressions.length} watch expressions`);
	return [...session.watchExpressions];
}

/**
 * Remove watch expressions from the session.
 * Accepts expressions to remove. Returns remaining watch list.
 */
export function removeSessionWatchExpressions(session: DebugSession, expressions: string[], logAction: (session: DebugSession, tool: string, summary: string) => void): string[] {
	session.watchExpressions = session.watchExpressions.filter((e) => !expressions.includes(e));
	logAction(session, "debug_unwatch", `Removed ${expressions.length} watch expression(s)`);
	return [...session.watchExpressions];
}

/**
 * Get the enriched session log with observations, compression, and token stats.
 */
export function getSessionLog(session: DebugSession, format: "summary" | "detailed"): string {
	const elapsedMs = Date.now() - session.startedAt;

	if (format === "detailed") {
		return formatSessionLogDetailed(session.actionLog, elapsedMs, session.tokenStats);
	}

	return formatSessionLogSummary(session.actionLog, 10, elapsedMs, session.tokenStats);
}

/**
 * Get captured output from the debuggee.
 */
export function getSessionOutput(session: DebugSession, stream: "stdout" | "stderr" | "both", sinceAction: number): string {
	const lines: string[] = [];

	if (stream === "stdout" || stream === "both") {
		for (const entry of session.outputBuffer.stdout) {
			if (entry.actionNumber >= sinceAction) {
				lines.push(stream === "both" ? `[stdout] ${entry.text}` : entry.text);
			}
		}
	}

	if (stream === "stderr" || stream === "both") {
		for (const entry of session.outputBuffer.stderr) {
			if (entry.actionNumber >= sinceAction) {
				lines.push(stream === "both" ? `[stderr] ${entry.text}` : entry.text);
			}
		}
	}

	return lines.join("");
}

/**
 * Register the DAP output event handler that captures stdout/stderr into the session buffer.
 */
export function registerOutputHandler(session: DebugSession, dapClient: DAPClient): void {
	dapClient.on("output", (event) => {
		const body = (event as DebugProtocol.OutputEvent).body;
		const category = body.category ?? "console";
		const text = body.output;
		const entry = { text, actionNumber: session.actionCount };

		if (category === "stdout") {
			session.outputBuffer.stdout.push(entry);
		} else if (category === "stderr") {
			session.outputBuffer.stderr.push(entry);
		}
		// "console" category is debugger-internal output, skip

		session.outputBuffer.totalBytes += Buffer.byteLength(text);

		// Truncation: keep tail when over limit
		while (session.outputBuffer.totalBytes > session.limits.maxOutputBytes) {
			const target = session.outputBuffer.stdout.length >= session.outputBuffer.stderr.length ? session.outputBuffer.stdout : session.outputBuffer.stderr;
			if (target.length === 0) break;
			const removed = target.shift();
			if (removed) session.outputBuffer.totalBytes -= Buffer.byteLength(removed.text);
		}
	});
}
