import { z } from "zod";
import type { ActionObservationKind, SessionStatus, StopReason } from "./enums.js";

export type { SessionStatus, StopReason } from "./enums.js";

// --- Viewport Configuration ---

export const ViewportConfigSchema = z.object({
	sourceContextLines: z.number().default(15),
	stackDepth: z.number().default(5),
	localsMaxDepth: z.number().default(1),
	localsMaxItems: z.number().default(20),
	stringTruncateLength: z.number().default(120),
	collectionPreviewItems: z.number().default(5),
});

export type ViewportConfig = z.infer<typeof ViewportConfigSchema>;

/** Partial viewport config schema — all fields optional. Used by daemon protocol and MCP tools. */
export const ViewportConfigPartialSchema = ViewportConfigSchema.partial();

/**
 * Map the MCP tool's snake_case viewport_config input to the camelCase
 * ViewportConfig expected by SessionManager. Returns undefined if not provided.
 * Note: the MCP ViewportConfigSchema uses snake_case keys while the core schema
 * uses camelCase — this function bridges that intentional divergence.
 */
export function mapViewportConfig(
	viewport_config:
		| {
				source_context_lines?: number;
				stack_depth?: number;
				locals_max_depth?: number;
				locals_max_items?: number;
				string_truncate_length?: number;
				collection_preview_items?: number;
		  }
		| undefined,
): Partial<ViewportConfig> | undefined {
	if (!viewport_config) return undefined;
	return {
		sourceContextLines: viewport_config.source_context_lines,
		stackDepth: viewport_config.stack_depth,
		localsMaxDepth: viewport_config.locals_max_depth,
		localsMaxItems: viewport_config.locals_max_items,
		stringTruncateLength: viewport_config.string_truncate_length,
		collectionPreviewItems: viewport_config.collection_preview_items,
	};
}

// --- Breakpoints ---

export const BreakpointSchema = z.object({
	line: z.number().describe("Line number"),
	condition: z.string().optional().describe("Expression that must be true to trigger. E.g., 'discount < 0'"),
	hitCondition: z.string().optional().describe("Break after N hits. E.g., '>=100'"),
	logMessage: z.string().optional().describe("Log instead of breaking. Supports {expression} interpolation."),
});

export type Breakpoint = z.infer<typeof BreakpointSchema>;

/**
 * Schema for a set of breakpoints in a single file.
 * Used by both the daemon protocol and MCP tools.
 */
export const FileBreakpointsSchema = z.object({
	file: z.string().describe("Source file path (relative or absolute)"),
	breakpoints: z.array(BreakpointSchema),
});

export type FileBreakpoints = z.infer<typeof FileBreakpointsSchema>;

// --- Session ---

export interface SessionInfo {
	sessionId: string;
	status: SessionStatus;
	language: string;
	startedAt: number;
	actionCount: number;
}

// --- Resource Limits ---

export const ResourceLimitsSchema = z.object({
	sessionTimeoutMs: z.number().default(300_000),
	maxActionsPerSession: z.number().default(200),
	maxConcurrentSessions: z.number().default(3),
	stepTimeoutMs: z.number().default(30_000),
	maxOutputBytes: z.number().default(1_048_576),
	maxEvaluateTimeMs: z.number().default(5_000),
});

export type ResourceLimits = z.infer<typeof ResourceLimitsSchema>;

// --- Viewport Snapshot ---

export interface StackFrame {
	file: string;
	shortFile: string;
	line: number;
	function: string;
	arguments: string;
}

export interface SourceLine {
	line: number;
	text: string;
}

export interface Variable {
	name: string;
	value: string;
	type?: string;
}

export interface ExceptionInfo {
	/** Exception type name, e.g. "ValueError", "TypeError" */
	type: string;
	/** Exception message */
	message: string;
	/** Exception ID from DAP (for drill-down) */
	exceptionId?: string;
}

export interface ThreadInfo {
	/** DAP thread ID */
	id: number;
	/** Thread name (e.g., "MainThread", "goroutine 1") */
	name: string;
	/** Whether this thread is currently stopped */
	stopped: boolean;
}

export interface ViewportSnapshot {
	file: string;
	line: number;
	function: string;
	reason: StopReason;
	stack: StackFrame[];
	totalFrames: number;
	source: SourceLine[];
	locals: Variable[];
	watches?: Variable[];
	/** Exception info when stopped on exception */
	exception?: ExceptionInfo;
	/** Thread info when multiple threads exist */
	thread?: {
		id: number;
		name: string;
		totalThreads: number;
	};
	/** Compression note appended to viewport when active */
	compressionNote?: string;
}

// --- Compression Tiers ---

export interface CompressionTier {
	/** Action count threshold to activate this tier */
	minActions: number;
	/** Override viewport config for this tier */
	overrides: Partial<ViewportConfig>;
	/** Enable diff mode automatically at this tier */
	diffMode: boolean;
}

export const DEFAULT_COMPRESSION_TIERS: CompressionTier[] = [
	// Tier 0: actions 1–20 — full viewport (defaults)
	{ minActions: 0, overrides: {}, diffMode: false },
	// Tier 1: actions 21–50 — moderate compression
	{ minActions: 21, overrides: { stackDepth: 3, stringTruncateLength: 80, collectionPreviewItems: 3 }, diffMode: false },
	// Tier 2: actions 51–100 — heavy compression + auto diff
	{ minActions: 51, overrides: { stackDepth: 2, stringTruncateLength: 60, collectionPreviewItems: 2, localsMaxItems: 10 }, diffMode: true },
	// Tier 3: actions 100+ — minimal viewport
	{ minActions: 100, overrides: { stackDepth: 1, stringTruncateLength: 40, collectionPreviewItems: 1, localsMaxItems: 5, sourceContextLines: 7 }, diffMode: true },
];

// --- Enriched Action Log ---

export interface ActionObservation {
	/** Type of observation */
	kind: ActionObservationKind;
	/** Human-readable description */
	description: string;
}

export interface EnrichedActionLogEntry {
	actionNumber: number;
	tool: string;
	/** Key parameters (e.g., expression for evaluate, direction for step) */
	keyParams: Record<string, unknown>;
	summary: string;
	timestamp: number;
	/** Extracted observations from viewport at this action */
	observations: ActionObservation[];
	/** Location at this action (file:line function) */
	location?: string;
}

// --- Viewport Diff ---

export interface VariableChange {
	name: string;
	oldValue: string;
	newValue: string;
}

export interface ViewportDiff {
	/** True if this is a diff (same file + function as previous) */
	isDiff: true;
	file: string;
	line: number;
	function: string;
	reason: StopReason;
	/** Only variables whose values changed */
	changedVariables: VariableChange[];
	/** Count of unchanged variables */
	unchangedCount: number;
	/** New or removed stack frames relative to previous */
	stackChanges?: { added: StackFrame[]; removed: StackFrame[] };
	/** Source context only if current line moved out of previous window */
	source?: SourceLine[];
	/** Watch expression results (always included) */
	watches?: Variable[];
	/** Compression note if active */
	compressionNote?: string;
}

// --- Token Tracking ---

export interface TokenStats {
	/** Estimated tokens consumed by viewport output across session */
	viewportTokensConsumed: number;
	/** Number of viewports rendered */
	viewportCount: number;
}
