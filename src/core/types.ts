import { z } from "zod";

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

// --- Breakpoints ---

export const BreakpointSchema = z.object({
	line: z.number(),
	condition: z.string().optional(),
	hitCondition: z.string().optional(),
	logMessage: z.string().optional(),
});

export type Breakpoint = z.infer<typeof BreakpointSchema>;

// --- Session ---

export type SessionStatus = "running" | "stopped" | "terminated" | "error";

export type StopReason = "breakpoint" | "step" | "exception" | "pause" | "entry";

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
}
