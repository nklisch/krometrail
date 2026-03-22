import { toSourceBreakpoints } from "./breakpoint-manager.js";
import type { StopResult } from "./dap-client.js";
import type { StepDirection } from "./enums.js";
import type { DebugSession } from "./session-manager.js";

/**
 * Get the thread ID for the session.
 * Falls back to 1 (DAP convention for single-threaded programs with no explicit thread).
 */
export function getThreadId(session: DebugSession): number {
	return session.lastStoppedThreadId ?? 1;
}

/**
 * Execute DAP continue logic and return a StopResult promise.
 * Handles the stopped vs running state difference.
 */
export async function executeContinue(session: DebugSession, timeoutMs: number, threadId?: number): Promise<StopResult> {
	if (session.state === "stopped") {
		const tid = threadId ?? getThreadId(session);
		await session.dapClient.continue(tid);
		session.state = "running";
		return session.dapClient.waitForStop(timeoutMs);
	}

	// Use pendingStopPromise if registered during launch to avoid race conditions,
	// otherwise fall back to a fresh waitForStop.
	const promise = session.pendingStopPromise ?? session.dapClient.waitForStop(timeoutMs);
	session.pendingStopPromise = null;
	return promise;
}

/**
 * Execute DAP step logic for count steps, returning the last rendered viewport.
 */
export async function executeStep(
	session: DebugSession,
	direction: StepDirection,
	count: number,
	stepTimeoutMs: number,
	threadId: number | undefined,
	handleStopResult: (session: DebugSession, stopResult: StopResult) => Promise<string>,
	checkAndIncrementAction: (session: DebugSession, toolName: string) => void,
): Promise<string> {
	let viewport = "";
	for (let i = 0; i < count; i++) {
		checkAndIncrementAction(session, "debug_step");
		const tid = threadId ?? getThreadId(session);

		if (direction === "over") {
			await session.dapClient.next(tid);
		} else if (direction === "into") {
			await session.dapClient.stepIn(tid);
		} else {
			await session.dapClient.stepOut(tid);
		}

		session.state = "running";
		const stopResult = await session.dapClient.waitForStop(stepTimeoutMs);
		viewport = await handleStopResult(session, stopResult);

		if ((session.state as string) === "terminated") break;
	}

	return viewport;
}

/**
 * Execute runTo logic: sets a temp breakpoint, continues, then restores original breakpoints.
 */
export async function executeRunTo(
	session: DebugSession,
	file: string,
	line: number,
	timeoutMs: number,
	handleStopResult: (session: DebugSession, stopResult: StopResult) => Promise<string>,
): Promise<string> {
	const { resolve } = await import("node:path");
	const absFile = resolve(process.cwd(), file);
	const existing = session.breakpointMap.get(absFile) ?? [];

	// Add temp breakpoint
	const allBps = [...existing, { line } as import("./types.js").Breakpoint];
	await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(allBps));

	// Continue
	const threadId = getThreadId(session);
	await session.dapClient.continue(threadId);
	session.state = "running";

	const stopResult = await session.dapClient.waitForStop(timeoutMs);
	const viewport = await handleStopResult(session, stopResult);

	// Restore original breakpoints (remove temp)
	await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(existing));

	return viewport;
}
