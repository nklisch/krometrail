import type { SessionManager } from "../core/session-manager.js";
import {
	AttachParamsSchema,
	ContinueParamsSchema,
	EvaluateParamsSchema,
	LaunchParamsSchema,
	OutputParamsSchema,
	RunToParamsSchema,
	SessionIdParamsSchema,
	SessionLogParamsSchema,
	SetBreakpointsParamsSchema,
	SetExceptionBreakpointsParamsSchema,
	SourceParamsSchema,
	StackTraceParamsSchema,
	StepParamsSchema,
	UnwatchParamsSchema,
	VariablesParamsSchema,
	WatchParamsSchema,
} from "./protocol.js";

export async function handleSessionMethod(method: string, params: Record<string, unknown>, sessionManager: SessionManager): Promise<unknown> {
	switch (method) {
		// --- Session Lifecycle ---
		case "session.launch": {
			const p = LaunchParamsSchema.parse(params);
			return sessionManager.launch(p);
		}

		case "session.attach": {
			const p = AttachParamsSchema.parse(params);
			return sessionManager.attach(p);
		}

		case "session.stop": {
			const p = SessionIdParamsSchema.parse(params);
			return sessionManager.stop(p.sessionId);
		}

		case "session.status": {
			const p = SessionIdParamsSchema.parse(params);
			return sessionManager.getStatus(p.sessionId);
		}

		// --- Execution Control ---
		case "session.continue": {
			const p = ContinueParamsSchema.parse(params);
			const viewport = await sessionManager.continue(p.sessionId, p.timeoutMs, p.threadId);
			return { viewport };
		}

		case "session.step": {
			const p = StepParamsSchema.parse(params);
			const viewport = await sessionManager.step(p.sessionId, p.direction, p.count, p.threadId);
			return { viewport };
		}

		case "session.runTo": {
			const p = RunToParamsSchema.parse(params);
			const viewport = await sessionManager.runTo(p.sessionId, p.file, p.line, p.timeoutMs);
			return { viewport };
		}

		// --- Breakpoints ---
		case "session.setBreakpoints": {
			const p = SetBreakpointsParamsSchema.parse(params);
			const verifiedBps = await sessionManager.setBreakpoints(p.sessionId, p.file, p.breakpoints);
			return { breakpoints: verifiedBps };
		}

		case "session.setExceptionBreakpoints": {
			const p = SetExceptionBreakpointsParamsSchema.parse(params);
			await sessionManager.setExceptionBreakpoints(p.sessionId, p.filters);
			return null;
		}

		case "session.listBreakpoints": {
			const p = SessionIdParamsSchema.parse(params);
			const bpMap = sessionManager.listBreakpoints(p.sessionId);
			const files: Record<string, Array<{ line: number; condition?: string; hitCondition?: string; logMessage?: string }>> = {};
			for (const [file, bps] of bpMap) {
				files[file] = bps.map((bp) => ({
					line: bp.line,
					...(bp.condition !== undefined && { condition: bp.condition }),
					...(bp.hitCondition !== undefined && { hitCondition: bp.hitCondition }),
					...(bp.logMessage !== undefined && { logMessage: bp.logMessage }),
				}));
			}
			return { files };
		}

		// --- State Inspection ---
		case "session.evaluate": {
			const p = EvaluateParamsSchema.parse(params);
			return sessionManager.evaluate(p.sessionId, p.expression, p.frameIndex, p.maxDepth);
		}

		case "session.variables": {
			const p = VariablesParamsSchema.parse(params);
			return sessionManager.getVariables(p.sessionId, p.scope, p.frameIndex, p.filter, p.maxDepth);
		}

		case "session.stackTrace": {
			const p = StackTraceParamsSchema.parse(params);
			return sessionManager.getStackTrace(p.sessionId, p.maxFrames, p.includeSource);
		}

		case "session.source": {
			const p = SourceParamsSchema.parse(params);
			return sessionManager.getSource(p.sessionId, p.file, p.startLine, p.endLine);
		}

		// --- Session Intelligence ---
		case "session.watch": {
			const p = WatchParamsSchema.parse(params);
			return sessionManager.addWatchExpressions(p.sessionId, p.expressions);
		}

		case "session.unwatch": {
			const p = UnwatchParamsSchema.parse(params);
			return sessionManager.removeWatchExpressions(p.sessionId, p.expressions);
		}

		case "session.sessionLog": {
			const p = SessionLogParamsSchema.parse(params);
			return sessionManager.getSessionLog(p.sessionId, p.format);
		}

		case "session.output": {
			const p = OutputParamsSchema.parse(params);
			return sessionManager.getOutput(p.sessionId, p.stream, p.sinceAction);
		}

		case "session.threads": {
			const p = SessionIdParamsSchema.parse(params);
			return sessionManager.getThreads(p.sessionId);
		}

		default:
			return undefined; // not handled
	}
}
