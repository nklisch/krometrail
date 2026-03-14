import type { SessionSummary } from "../browser/investigation/query-engine.js";
import type { BrowserSessionInfo } from "../browser/types.js";
import type { BreakpointsListPayload, BreakpointsResultPayload, LaunchResultPayload, StatusResultPayload, StopResultPayload, ThreadInfoPayload } from "../daemon/protocol.js";
import { errorEnvelope, successEnvelope } from "./envelope.js";

/**
 * Output mode determined by CLI flags.
 */
export type OutputMode = "text" | "json" | "quiet";

/**
 * Resolve output mode from CLI flags.
 */
export function resolveOutputMode(flags: { json?: boolean; quiet?: boolean }): OutputMode {
	if (flags.json) return "json";
	if (flags.quiet) return "quiet";
	return "text";
}

// --- Data shape interfaces (documentation of what agents receive) ---

export interface LaunchData {
	sessionId: string;
	status: string;
	framework?: string;
	frameworkWarnings?: string[];
	viewport?: string;
}

export interface StopData {
	sessionId: string;
	durationMs: number;
	durationSec: number;
	actionCount: number;
}

export interface StatusData {
	status: string;
	viewport?: string;
	tokenStats?: { viewportTokensConsumed: number; viewportCount: number };
	actionCount?: number;
	elapsedMs?: number;
}

export interface ViewportData {
	viewport: string;
}

export interface EvalData {
	expression: string;
	result: string;
}

export interface VariablesData {
	variables: string;
}

export interface StackTraceData {
	stackTrace: string;
}

export interface BreakpointsSetData {
	file: string;
	breakpoints: BreakpointsResultPayload["breakpoints"];
}

export interface WatchData {
	watchExpressions: string[];
	count: number;
}

export interface ThreadsData {
	threads: ThreadInfoPayload[];
	count: number;
}

export interface SourceData {
	file: string;
	source: string;
}

export interface LogData {
	log: string;
}

export interface OutputData {
	output: string;
	stream: string;
}

export interface BrowserSessionData {
	startedAt: string;
	eventCount: number;
	markerCount: number;
	bufferAgeMs: number;
	tabs: Array<{ url: string; title: string }>;
}

export interface BrowserMarkData {
	id: string;
	timestamp: string;
	label?: string;
}

export interface InvestigationData {
	result: string;
	command: string;
}

// --- Format functions ---

/**
 * Format a launch result for CLI output.
 */
export function formatLaunch(result: LaunchResultPayload, mode: OutputMode): string {
	if (mode === "json") {
		const data: LaunchData = {
			sessionId: result.sessionId,
			status: result.status,
			framework: result.framework,
			frameworkWarnings: result.frameworkWarnings,
			viewport: result.viewport,
		};
		return successEnvelope(data);
	}
	if (mode === "quiet") {
		return result.viewport ?? "";
	}
	// text mode
	const lines: string[] = [`Session started: ${result.sessionId}`];
	if (result.framework) lines.push(`Framework: ${result.framework}`);
	if (result.frameworkWarnings?.length) for (const w of result.frameworkWarnings) lines.push(`Warning: ${w}`);
	if (result.viewport) {
		lines.push(result.viewport);
	} else {
		lines.push(`Status: ${result.status}`);
	}
	return lines.join("\n");
}

/**
 * Format a stop result for CLI output.
 */
export function formatStop(result: StopResultPayload, sessionId: string, mode: OutputMode): string {
	if (mode === "json") {
		const durationSec = result.duration / 1000;
		return successEnvelope<StopData>({ sessionId, durationMs: result.duration, durationSec, actionCount: result.actionCount });
	}
	if (mode === "quiet") {
		return "";
	}
	const durationSec = (result.duration / 1000).toFixed(1);
	return `Session ${sessionId} ended. Duration: ${durationSec}s, Actions: ${result.actionCount}`;
}

/**
 * Format a status result for CLI output.
 */
export function formatStatus(result: StatusResultPayload, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<StatusData>(result as StatusData);
	}
	if (mode === "quiet") {
		return result.viewport ?? result.status;
	}
	const lines: string[] = [`Status: ${result.status}`];
	if (result.viewport) {
		lines.push(result.viewport);
	}
	return lines.join("\n");
}

/**
 * Format a viewport string for CLI output.
 * In text mode: print as-is.
 * In quiet mode: print as-is (viewport already is the minimal form).
 * In JSON mode: wrap in success envelope with viewport field.
 */
export function formatViewport(viewport: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<ViewportData>({ viewport });
	}
	return viewport;
}

/**
 * Format an evaluate result.
 */
export function formatEvaluate(expression: string, result: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<EvalData>({ expression, result });
	}
	if (mode === "quiet") {
		return result;
	}
	return `${expression} = ${result}`;
}

/**
 * Format a variables result.
 */
export function formatVariables(result: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<VariablesData>({ variables: result });
	}
	return result;
}

/**
 * Format a stack trace result.
 */
export function formatStackTrace(result: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<StackTraceData>({ stackTrace: result });
	}
	return result;
}

/**
 * Format a breakpoint set result.
 */
export function formatBreakpointsSet(file: string, result: BreakpointsResultPayload, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<BreakpointsSetData>({ file, breakpoints: result.breakpoints });
	}
	if (mode === "quiet") {
		return result.breakpoints.map((bp) => `${file}:${bp.requestedLine} ${bp.verified ? "✓" : "✗"}`).join("\n");
	}
	const lines: string[] = [`Breakpoints set in ${file}:`];
	for (const bp of result.breakpoints) {
		const adjustedNote = bp.verifiedLine !== null && bp.verifiedLine !== bp.requestedLine ? ` → adjusted to line ${bp.verifiedLine}` : "";
		const status = bp.verified ? `verified${adjustedNote}` : `unverified${bp.message ? ` (${bp.message})` : ""}`;
		lines.push(`  Line ${bp.requestedLine}: ${status}`);
	}
	return lines.join("\n");
}

/**
 * Format a breakpoint list result.
 */
export function formatBreakpointsList(result: BreakpointsListPayload, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope(result);
	}
	const files = Object.entries(result.files);
	if (files.length === 0) {
		return "No breakpoints set.";
	}
	const lines: string[] = [];
	for (const [file, bps] of files) {
		lines.push(`${file}:`);
		for (const bp of bps) {
			let desc = `  Line ${bp.line}`;
			if (bp.condition) desc += ` when ${bp.condition}`;
			if (bp.hitCondition) desc += ` hit ${bp.hitCondition}`;
			if (bp.logMessage) desc += ` log '${bp.logMessage}'`;
			lines.push(desc);
		}
	}
	return lines.join("\n");
}

/**
 * Format watch expressions for CLI output.
 */
export function formatWatchExpressions(expressions: string[], mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<WatchData>({ watchExpressions: expressions, count: expressions.length });
	}
	const lines: string[] = [`Watch expressions (${expressions.length} total):`];
	for (const expr of expressions) lines.push(`  ${expr}`);
	return lines.join("\n");
}

/**
 * Format a threads list for CLI output.
 */
export function formatThreads(threads: ThreadInfoPayload[], mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<ThreadsData>({ threads, count: threads.length });
	}
	const lines: string[] = [`Threads (${threads.length}):`];
	for (const t of threads) {
		lines.push(`  ${t.stopped ? "→" : " "} Thread ${t.id}: ${t.name}${t.stopped ? " (stopped)" : " (running)"}`);
	}
	return lines.join("\n");
}

/**
 * Format a source code view for CLI output.
 */
export function formatSource(file: string, source: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<SourceData>({ file, source });
	}
	return source;
}

/**
 * Format a session log for CLI output.
 */
export function formatLog(log: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<LogData>({ log });
	}
	return log;
}

/**
 * Format program output for CLI output.
 */
export function formatOutput(output: string, stream: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<OutputData>({ output, stream });
	}
	return output || "No output captured.";
}

/**
 * Format browser session info for CLI output.
 * Used by browser start, status, and mark commands.
 */
export function formatBrowserSession(info: BrowserSessionInfo, mode: OutputMode): string {
	if (mode === "json") {
		const data: BrowserSessionData = {
			startedAt: new Date(info.startedAt).toISOString(),
			eventCount: info.eventCount,
			markerCount: info.markerCount,
			bufferAgeMs: info.bufferAgeMs,
			tabs: info.tabs.map((t) => ({ url: t.url, title: t.title })),
		};
		return successEnvelope<BrowserSessionData>(data);
	}
	// text/quiet mode
	const lines: string[] = [];
	const startedAt = new Date(info.startedAt).toLocaleTimeString();
	lines.push(`Browser recording active since ${startedAt}`);
	lines.push(`Events: ${info.eventCount}  Markers: ${info.markerCount}  Buffer age: ${Math.round(info.bufferAgeMs / 1000)}s`);
	if (info.tabs.length > 0) {
		lines.push("Tabs:");
		for (const tab of info.tabs) {
			const title = tab.title ? `"${tab.title}" ` : "";
			lines.push(`  ${title}(${tab.url})`);
		}
	}
	return lines.join("\n");
}

/**
 * Format a list of browser session summaries for CLI output.
 */
export function formatBrowserSessions(sessions: SessionSummary[], mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope({ sessions, count: sessions.length });
	}
	if (sessions.length === 0) {
		return "No recorded sessions found.";
	}
	const lines: string[] = [`Sessions (${sessions.length}):`];
	for (const s of sessions) {
		const seconds = Math.floor(s.duration / 1000);
		const duration = seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
		const markers = s.markerCount > 0 ? `, ${s.markerCount} markers` : "";
		const errors = s.errorCount > 0 ? `, ${s.errorCount} errors` : "";
		const startedAt = new Date(s.startedAt).toISOString().slice(11, 23);
		lines.push(`  ${s.id}  ${startedAt}  ${duration}  ${s.url}  (${s.eventCount} events${markers}${errors})`);
	}
	return lines.join("\n");
}

/**
 * Format a browser investigation result (overview, search, inspect, diff, replay-context).
 * In json mode, wraps the result string in the envelope under `result`.
 * In text/quiet mode, returns result as-is.
 */
export function formatInvestigation(result: string, command: string, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope<InvestigationData>({ result, command });
	}
	return result;
}

/**
 * Format an error for CLI output.
 * In json mode: errorEnvelope wrapping { ok: false, error: { code, message, retryable } }
 * In text/quiet mode: "Error: <message>"
 */
export function formatError(err: unknown, mode: OutputMode): string {
	if (mode === "json") {
		return errorEnvelope(err);
	}
	const message = err instanceof Error ? err.message : String(err);
	return `Error: ${message}`;
}
