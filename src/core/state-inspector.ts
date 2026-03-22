import { readFileSync } from "node:fs";
import { SessionStateError } from "./errors.js";
import type { DebugSession } from "./session-manager.js";
import { convertDAPVariables, renderDAPVariable } from "./value-renderer.js";
import { renderAlignedVariables } from "./viewport.js";

/**
 * Read source file from disk, using per-session cache.
 */
export async function readSourceFile(session: DebugSession, filePath: string): Promise<string[]> {
	const cached = session.sourceCache.get(filePath);
	if (cached) return cached;

	const text = readFileSync(filePath, "utf-8");
	const lines = text.split("\n");
	session.sourceCache.set(filePath, lines);
	return lines;
}

/**
 * Get frameId for a given frame index.
 */
export async function resolveFrameId(session: DebugSession, frameIndex: number, getThreadId: (session: DebugSession) => number): Promise<number> {
	if (frameIndex === 0 && session.lastStoppedFrameId !== null) {
		return session.lastStoppedFrameId;
	}
	const threadId = getThreadId(session);
	const response = await session.dapClient.stackTrace(threadId, 0, frameIndex + 1);
	const frames = response.body?.stackFrames ?? [];
	if (!frames[frameIndex]) throw new SessionStateError(session.id, `no-frame-at-index-${frameIndex}`, ["stopped"]);
	return frames[frameIndex].id;
}

/**
 * Evaluate an expression in a stack frame.
 */
export async function evaluateExpression(
	session: DebugSession,
	expression: string,
	frameIndex: number,
	maxDepth: number,
	getThreadId: (session: DebugSession) => number,
	logAction: (session: DebugSession, tool: string, summary: string) => void,
): Promise<string> {
	const frameId = await resolveFrameId(session, frameIndex, getThreadId);
	const response = await session.dapClient.evaluate(expression, frameId, "repl");

	const rendered = renderDAPVariable(
		{
			name: expression,
			value: response.body.result,
			type: response.body.type,
			variablesReference: response.body.variablesReference,
			evaluateName: expression,
			presentationHint: undefined,
			namedVariables: undefined,
			indexedVariables: undefined,
			memoryReference: undefined,
		},
		{
			depth: 0,
			maxDepth,
			stringTruncateLength: session.viewportConfig.stringTruncateLength,
			collectionPreviewItems: session.viewportConfig.collectionPreviewItems,
		},
	);

	logAction(session, "debug_evaluate", `Evaluated: ${expression} = ${rendered}`);
	return rendered;
}

/**
 * Get variables for a scope.
 */
export async function getSessionVariables(
	session: DebugSession,
	scope: "local" | "global" | "closure" | "all",
	frameIndex: number,
	filter: string | undefined,
	maxDepth: number,
	getThreadId: (session: DebugSession) => number,
): Promise<string> {
	const frameId = await resolveFrameId(session, frameIndex, getThreadId);
	const scopesResponse = await session.dapClient.scopes(frameId);
	const scopes = scopesResponse.body?.scopes ?? [];

	const targetScopes =
		scope === "all"
			? scopes
			: scopes.filter((s) => {
					const name = s.name.toLowerCase();
					// Prefix matching handles adapters that append context (e.g. js-debug: "Local: main", "Block: main").
					if (scope === "local") return name.startsWith("local") || name.startsWith("block");
					if (scope === "global") return name.startsWith("global");
					if (scope === "closure") return name.startsWith("closure") || name === "free variables";
					return false;
				});

	const lines: string[] = [];
	const filterRegex = filter ? new RegExp(filter) : null;

	for (const s of targetScopes) {
		const varsResponse = await session.dapClient.variables(s.variablesReference);
		const vars = varsResponse.body?.variables ?? [];
		const converted = convertDAPVariables(vars, session.viewportConfig).filter((v) => !filterRegex || filterRegex.test(v.name));

		if (scope === "all" && converted.length > 0) {
			lines.push(`[${s.name}]`);
		}

		for (const line of renderAlignedVariables(converted, 4)) {
			lines.push(line);
		}
	}

	void maxDepth;
	return lines.join("\n");
}

/**
 * Get the full stack trace.
 */
export async function getSessionStackTrace(session: DebugSession, maxFrames: number, includeSource: boolean, getThreadId: (session: DebugSession) => number): Promise<string> {
	const threadId = getThreadId(session);
	const response = await session.dapClient.stackTrace(threadId, 0, maxFrames);
	const frames = response.body?.stackFrames ?? [];

	const lines: string[] = [];
	for (let i = 0; i < frames.length; i++) {
		const f = frames[i];
		const marker = i === 0 ? "→" : " ";
		const file = f.source?.path ?? f.source?.name ?? "<unknown>";
		const shortFile = file.split("/").pop() ?? file;
		lines.push(`${marker} #${i} ${shortFile}:${f.line}  ${f.name}()`);

		if (includeSource && f.source?.path) {
			try {
				const sourceLines = await readSourceFile(session, f.source.path);
				const start = Math.max(0, f.line - 2);
				const end = Math.min(sourceLines.length - 1, f.line + 1);
				for (let l = start; l <= end; l++) {
					const arrow = l + 1 === f.line ? "→" : " ";
					lines.push(`    ${arrow}${String(l + 1).padStart(4)}│ ${sourceLines[l]}`);
				}
			} catch {
				// Skip source if unavailable
			}
		}
	}

	return lines.join("\n");
}

/**
 * Read source file, return numbered lines for the requested range.
 */
export async function getSessionSource(session: DebugSession, file: string, startLine: number, endLine: number | undefined): Promise<string> {
	const lines = await readSourceFile(session, file);
	const end = endLine ?? startLine + 40;
	const slice = lines.slice(startLine - 1, end);

	return slice.map((text, i) => `${String(startLine + i).padStart(4)}│ ${text}`).join("\n");
}
