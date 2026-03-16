import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { z } from "zod";
import type { BrowserSessionInfo, Marker } from "../browser/types.js";
import {
	DiffIncludeSchema,
	ExportFormatSchema,
	FrameworkSchema,
	InspectIncludeSchema,
	OutputStreamSchema,
	OverviewIncludeSchema,
	ReplayFormatSchema,
	SessionLogFormatSchema,
	StepDirectionSchema,
	TestFrameworkSchema,
	VariableScopeSchema,
	ViewportConfigPartialSchema,
} from "../core/enums.js";
import { getKrometrailDir } from "../core/paths.js";
import { BreakpointSchema, FileBreakpointsSchema } from "../core/types.js";

// --- JSON-RPC 2.0 Base Types ---

export interface JsonRpcRequest {
	jsonrpc: "2.0";
	id: number;
	method: string;
	params?: Record<string, unknown>;
}

export interface JsonRpcResponse {
	jsonrpc: "2.0";
	id: number;
	result?: unknown;
	error?: JsonRpcError;
}

export interface JsonRpcError {
	code: number;
	message: string;
	data?: unknown;
}

// Standard JSON-RPC error codes
export const RPC_PARSE_ERROR = -32700;
export const RPC_INVALID_REQUEST = -32600;
export const RPC_METHOD_NOT_FOUND = -32601;
export const RPC_INVALID_PARAMS = -32602;
export const RPC_INTERNAL_ERROR = -32603;

// Application error codes (krometrail specific)
export const RPC_SESSION_NOT_FOUND = -32000;
export const RPC_SESSION_STATE_ERROR = -32001;
export const RPC_SESSION_LIMIT_ERROR = -32002;
export const RPC_ADAPTER_ERROR = -32003;
export const RPC_LAUNCH_ERROR = -32004;

// --- RPC Method Definitions ---

/**
 * Maps each RPC method name to its params and result types.
 * The daemon dispatches based on method name and validates params with Zod.
 */
export type RpcMethods = {
	// Lifecycle
	"session.launch": { params: LaunchParams; result: LaunchResultPayload };
	"session.attach": { params: AttachParams; result: LaunchResultPayload };
	"session.stop": { params: SessionIdParams; result: StopResultPayload };
	"session.status": { params: SessionIdParams; result: StatusResultPayload };

	// Execution control
	"session.continue": { params: ContinueParams; result: ViewportPayload };
	"session.step": { params: StepParams; result: ViewportPayload };
	"session.runTo": { params: RunToParams; result: ViewportPayload };

	// Breakpoints
	"session.setBreakpoints": { params: SetBreakpointsParams; result: BreakpointsResultPayload };
	"session.setExceptionBreakpoints": { params: SetExceptionBreakpointsParams; result: undefined };
	"session.listBreakpoints": { params: SessionIdParams; result: BreakpointsListPayload };

	// State inspection
	"session.evaluate": { params: EvaluateParams; result: string };
	"session.variables": { params: VariablesParams; result: string };
	"session.stackTrace": { params: StackTraceParams; result: string };
	"session.source": { params: SourceParams; result: string };

	// Session intelligence
	"session.watch": { params: WatchParams; result: string[] };
	"session.unwatch": { params: UnwatchParams; result: string[] };
	"session.sessionLog": { params: SessionLogParams; result: string };
	"session.output": { params: OutputParams; result: string };
	"session.threads": { params: SessionIdParams; result: ThreadInfoPayload[] };

	// Daemon control
	"daemon.ping": { params: undefined; result: { uptime: number; sessions: number } };
	"daemon.shutdown": { params: undefined; result: undefined };
	"daemon.sessions": {
		params: undefined;
		result: Array<{ id: string; status: string; language: string; actionCount: number }>;
	};

	// Browser recording
	"browser.start": { params: BrowserStartParams; result: BrowserSessionInfo };
	"browser.mark": { params: BrowserMarkParams; result: Marker };
	"browser.status": { params: Record<string, never>; result: BrowserSessionInfo | null };
	"browser.stop": { params: BrowserStopParams; result: undefined };

	// Browser step execution
	"browser.run-steps": { params: BrowserRunStepsParams; result: BrowserRunStepsResult };

	// Browser investigation
	"browser.sessions": { params: BrowserSessionsParams; result: unknown[] };
	"browser.overview": { params: BrowserOverviewParams; result: string };
	"browser.search": { params: BrowserSearchParams; result: string };
	"browser.inspect": { params: BrowserInspectParams; result: string };
	"browser.diff": { params: BrowserDiffParams; result: string };
	"browser.replay-context": { params: BrowserReplayContextParams; result: string };
	"browser.export": { params: BrowserExportParams; result: string };
};

// --- Param Schemas (Zod) ---

export const SessionIdParamsSchema = z.object({
	sessionId: z.string(),
});
export type SessionIdParams = z.infer<typeof SessionIdParamsSchema>;

export const LaunchParamsSchema = z.object({
	command: z.string(),
	language: z.string().optional(),
	framework: z.string().optional(),
	breakpoints: z.array(FileBreakpointsSchema).optional(),
	cwd: z.string().optional(),
	env: z.record(z.string(), z.string()).optional(),
	viewportConfig: ViewportConfigPartialSchema.optional(),
	stopOnEntry: z.boolean().optional(),
});
export type LaunchParams = z.infer<typeof LaunchParamsSchema>;

export const ContinueParamsSchema = z.object({
	sessionId: z.string(),
	timeoutMs: z.number().optional(),
	threadId: z.number().optional(),
});
export type ContinueParams = z.infer<typeof ContinueParamsSchema>;

export const StepParamsSchema = z.object({
	sessionId: z.string(),
	direction: StepDirectionSchema,
	count: z.number().optional(),
	threadId: z.number().optional(),
});
export type StepParams = z.infer<typeof StepParamsSchema>;

export const AttachParamsSchema = z.object({
	language: z.string(),
	pid: z.number().optional(),
	port: z.number().optional(),
	host: z.string().optional(),
	cwd: z.string().optional(),
	breakpoints: z.array(FileBreakpointsSchema).optional(),
	viewportConfig: ViewportConfigPartialSchema.optional(),
});
export type AttachParams = z.infer<typeof AttachParamsSchema>;

export const RunToParamsSchema = z.object({
	sessionId: z.string(),
	file: z.string(),
	line: z.number(),
	timeoutMs: z.number().optional(),
});
export type RunToParams = z.infer<typeof RunToParamsSchema>;

export const SetBreakpointsParamsSchema = z.object({
	sessionId: z.string(),
	file: z.string(),
	breakpoints: z.array(BreakpointSchema),
});
export type SetBreakpointsParams = z.infer<typeof SetBreakpointsParamsSchema>;

export const SetExceptionBreakpointsParamsSchema = z.object({
	sessionId: z.string(),
	filters: z.array(z.string()),
});
export type SetExceptionBreakpointsParams = z.infer<typeof SetExceptionBreakpointsParamsSchema>;

export const EvaluateParamsSchema = z.object({
	sessionId: z.string(),
	expression: z.string(),
	frameIndex: z.number().optional(),
	maxDepth: z.number().optional(),
});
export type EvaluateParams = z.infer<typeof EvaluateParamsSchema>;

export const VariablesParamsSchema = z.object({
	sessionId: z.string(),
	scope: VariableScopeSchema.optional(),
	frameIndex: z.number().optional(),
	filter: z.string().optional(),
	maxDepth: z.number().optional(),
});
export type VariablesParams = z.infer<typeof VariablesParamsSchema>;

export const StackTraceParamsSchema = z.object({
	sessionId: z.string(),
	maxFrames: z.number().optional(),
	includeSource: z.boolean().optional(),
});
export type StackTraceParams = z.infer<typeof StackTraceParamsSchema>;

export const SourceParamsSchema = z.object({
	sessionId: z.string(),
	file: z.string(),
	startLine: z.number().optional(),
	endLine: z.number().optional(),
});
export type SourceParams = z.infer<typeof SourceParamsSchema>;

export const WatchParamsSchema = z.object({
	sessionId: z.string(),
	expressions: z.array(z.string()),
});
export type WatchParams = z.infer<typeof WatchParamsSchema>;

export const UnwatchParamsSchema = z.object({
	sessionId: z.string(),
	expressions: z.array(z.string()),
});
export type UnwatchParams = z.infer<typeof UnwatchParamsSchema>;

export const SessionLogParamsSchema = z.object({
	sessionId: z.string(),
	format: SessionLogFormatSchema.optional(),
});
export type SessionLogParams = z.infer<typeof SessionLogParamsSchema>;

export const OutputParamsSchema = z.object({
	sessionId: z.string(),
	stream: OutputStreamSchema.optional(),
	sinceAction: z.number().optional(),
});
export type OutputParams = z.infer<typeof OutputParamsSchema>;

// --- Browser Param Schemas ---

export const FrameworkStateConfigSchema = z.union([z.boolean(), z.array(FrameworkSchema)]).optional();

export const BrowserStartParamsSchema = z.object({
	port: z.number().default(9222),
	profile: z.string().optional(),
	attach: z.boolean().default(false),
	allTabs: z.boolean().default(false),
	tabFilter: z.string().optional(),
	url: z.string().optional(),
	screenshotIntervalMs: z.number().optional(),
	frameworkState: FrameworkStateConfigSchema,
});
export type BrowserStartParams = z.infer<typeof BrowserStartParamsSchema>;

export const BrowserMarkParamsSchema = z.object({
	label: z.string().optional(),
});
export type BrowserMarkParams = z.infer<typeof BrowserMarkParamsSchema>;

export const BrowserStopParamsSchema = z.object({
	closeBrowser: z.boolean().default(false),
});
export type BrowserStopParams = z.infer<typeof BrowserStopParamsSchema>;

export const BrowserSessionsParamsSchema = z.object({
	after: z.number().optional(),
	before: z.number().optional(),
	urlContains: z.string().optional(),
	hasMarkers: z.boolean().optional(),
	hasErrors: z.boolean().optional(),
	limit: z.number().int().positive().optional(),
});
export type BrowserSessionsParams = z.infer<typeof BrowserSessionsParamsSchema>;

export const BrowserOverviewParamsSchema = z.object({
	sessionId: z.string(),
	include: z.array(OverviewIncludeSchema).optional(),
	aroundMarker: z.string().optional(),
	timeRange: z.object({ start: z.number(), end: z.number() }).optional(),
	tokenBudget: z.number().optional(),
});
export type BrowserOverviewParams = z.infer<typeof BrowserOverviewParamsSchema>;

export const BrowserSearchParamsSchema = z.object({
	sessionId: z.string(),
	query: z.string().optional(),
	eventTypes: z.array(z.string()).optional(),
	statusCodes: z.array(z.number()).optional(),
	timeRange: z.object({ start: z.number(), end: z.number() }).optional(),
	maxResults: z.number().optional(),
	tokenBudget: z.number().optional(),
});
export type BrowserSearchParams = z.infer<typeof BrowserSearchParamsSchema>;

export const BrowserInspectParamsSchema = z.object({
	sessionId: z.string(),
	eventId: z.string().optional(),
	markerId: z.string().optional(),
	timestamp: z.union([z.string(), z.number()]).optional(),
	include: z.array(InspectIncludeSchema).optional(),
	contextWindow: z.number().optional(),
	tokenBudget: z.number().optional(),
});
export type BrowserInspectParams = z.infer<typeof BrowserInspectParamsSchema>;

export const BrowserDiffParamsSchema = z.object({
	sessionId: z.string(),
	before: z.string(),
	after: z.string(),
	include: z.array(DiffIncludeSchema).optional(),
	tokenBudget: z.number().optional(),
});
export type BrowserDiffParams = z.infer<typeof BrowserDiffParamsSchema>;

export const BrowserReplayContextParamsSchema = z.object({
	sessionId: z.string(),
	aroundMarker: z.string().optional(),
	timeRange: z.object({ start: z.number(), end: z.number() }).optional(),
	format: ReplayFormatSchema,
	testFramework: TestFrameworkSchema.optional(),
});
export type BrowserReplayContextParams = z.infer<typeof BrowserReplayContextParamsSchema>;

export const BrowserExportParamsSchema = z.object({
	sessionId: z.string(),
	format: ExportFormatSchema,
	timeRange: z.object({ start: z.number(), end: z.number() }).optional(),
	includeResponseBodies: z.boolean().optional(),
});
export type BrowserExportParams = z.infer<typeof BrowserExportParamsSchema>;

export type { RunStepsParams as BrowserRunStepsParams } from "../browser/executor/types.js";
export { RunStepsParamsSchema } from "../browser/executor/types.js";
export type BrowserRunStepsResult = import("../browser/executor/types.js").RunStepsResult;

// Re-export browser types for protocol consumers
export type { BrowserSessionInfo, Marker } from "../browser/types.js";

// --- Result Payloads ---

export interface LaunchResultPayload {
	sessionId: string;
	viewport?: string;
	status: string;
	/** Detected framework identifier (e.g., "pytest", "jest") */
	framework?: string;
	/** Warnings explaining framework-specific modifications */
	frameworkWarnings?: string[];
}

export interface StopResultPayload {
	duration: number;
	actionCount: number;
}

export interface StatusResultPayload {
	status: string;
	viewport?: string;
	tokenStats?: { viewportTokensConsumed: number; viewportCount: number };
	actionCount?: number;
	elapsedMs?: number;
}

export interface ViewportPayload {
	viewport: string;
}

export interface BreakpointsResultPayload {
	breakpoints: Array<{ requestedLine: number; verifiedLine: number | null; verified: boolean; message?: string; conditionAccepted?: boolean }>;
}

export interface ThreadInfoPayload {
	id: number;
	name: string;
	stopped: boolean;
}

export interface BreakpointsListPayload {
	files: Record<string, Array<{ line: number; condition?: string; hitCondition?: string; logMessage?: string }>>;
}

// --- Socket Path Resolution ---

/**
 * Resolve the daemon socket path.
 * Uses $XDG_RUNTIME_DIR/krometrail.sock if available,
 * falls back to ~/.krometrail/krometrail.sock.
 */
export function getDaemonSocketPath(): string {
	const xdgRuntime = process.env.XDG_RUNTIME_DIR;
	if (xdgRuntime) {
		return join(xdgRuntime, "krometrail.sock");
	}
	const dir = getKrometrailDir();
	mkdirSync(dir, { recursive: true });
	return join(dir, "krometrail.sock");
}

/**
 * Resolve the daemon PID file path (socket path + ".pid").
 */
export function getDaemonPidPath(): string {
	return `${getDaemonSocketPath()}.pid`;
}
