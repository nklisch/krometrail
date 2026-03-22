import { resolve } from "node:path";
import type { DebugProtocol } from "@vscode/debugprotocol";
import type { DebugSession } from "./session-manager.js";
import type { Breakpoint } from "./types.js";

// --- Interfaces ---

export interface VerifiedBreakpoint {
	/** Requested line */
	requestedLine: number;
	/** Actual line the debugger set the breakpoint on (may differ) */
	verifiedLine: number | null;
	/** Whether the debugger accepted the breakpoint */
	verified: boolean;
	/** Debugger message (e.g., "adjusted to nearest executable line") */
	message?: string;
	/** Whether the condition was accepted (null if no condition) */
	conditionAccepted?: boolean;
}

export interface SessionCapabilities {
	/** Whether conditional breakpoints are supported */
	supportsConditionalBreakpoints: boolean;
	/** Whether hit count breakpoints are supported */
	supportsHitConditionalBreakpoints: boolean;
	/** Whether logpoints are supported */
	supportsLogPoints: boolean;
	/** Whether exception info can be queried */
	supportsExceptionInfo: boolean;
	/** Available exception breakpoint filters */
	exceptionFilters: Array<{ filter: string; label: string }>;
	/** Whether the debugger supports restart */
	supportsRestart: boolean;
	/** Whether set-variable is supported */
	supportsSetVariable: boolean;
}

// --- Helpers ---

export function toSourceBreakpoints(bps: Breakpoint[]): DebugProtocol.SourceBreakpoint[] {
	return bps.map((bp) => ({
		line: bp.line,
		condition: bp.condition,
		hitCondition: bp.hitCondition,
		logMessage: bp.logMessage,
	}));
}

// --- Standalone Functions ---

/**
 * Set breakpoints in a file (DAP semantics: replaces all in that file).
 * Returns VerifiedBreakpoint[] with verification info from the debugger.
 */
export async function setSessionBreakpoints(
	session: DebugSession,
	file: string,
	breakpoints: Breakpoint[],
	logAction: (session: DebugSession, tool: string, summary: string) => void,
): Promise<VerifiedBreakpoint[]> {
	const absFile = resolve(process.cwd(), file);

	const response = await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(breakpoints));

	session.breakpointMap.set(absFile, breakpoints);
	logAction(session, "debug_set_breakpoints", `Set ${breakpoints.length} breakpoints in ${file}`);

	const verified = response.body?.breakpoints ?? [];
	return breakpoints.map((bp, i) => {
		const v = verified[i];
		return {
			requestedLine: bp.line,
			verifiedLine: v?.line ?? null,
			verified: v?.verified ?? false,
			message: v?.message,
			conditionAccepted: bp.condition ? (v?.verified ?? false) : undefined,
		};
	});
}

/**
 * Set exception breakpoint filters.
 */
export async function setSessionExceptionBreakpoints(session: DebugSession, filters: string[], logAction: (session: DebugSession, tool: string, summary: string) => void): Promise<void> {
	await session.dapClient.setExceptionBreakpoints(filters);
	logAction(session, "debug_set_exception_breakpoints", `Set exception filters: ${filters.join(", ")}`);
}

/**
 * Get the available exception breakpoint filters for a session.
 * Reads from DAP capabilities negotiated during initialize.
 */
export function getSessionExceptionBreakpointFilters(session: DebugSession): Array<{ filter: string; label: string; default?: boolean }> {
	const filters = session.dapClient.capabilities.exceptionBreakpointFilters ?? [];
	return filters.map((f) => ({
		filter: f.filter,
		label: f.label,
		default: f.default,
	}));
}

/**
 * Get structured capability info from DAP for this session.
 */
export function getSessionCapabilities(session: DebugSession): SessionCapabilities {
	const caps = session.dapClient.capabilities;
	return {
		supportsConditionalBreakpoints: caps.supportsConditionalBreakpoints ?? false,
		supportsHitConditionalBreakpoints: caps.supportsHitConditionalBreakpoints ?? false,
		supportsLogPoints: caps.supportsLogPoints ?? false,
		supportsExceptionInfo: caps.supportsExceptionInfoRequest ?? false,
		exceptionFilters: (caps.exceptionBreakpointFilters ?? []).map((f) => ({
			filter: f.filter,
			label: f.label,
		})),
		supportsRestart: caps.supportsRestartRequest ?? false,
		supportsSetVariable: caps.supportsSetVariable ?? false,
	};
}
