/**
 * Shared helpers for journey tests (Phase 18).
 * Extracted from duplicated helpers in agent-workflow.test.ts,
 * form-validation-bug.test.ts, session-lifecycle.test.ts, react-observer.test.ts.
 */

const UUID_RE = /[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}/gi;
const EVENT_ID_RE = /id[:\s]+"?([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})"?/gi;

/**
 * Extract a session ID (UUID) from session_list output.
 * Throws with context if not found.
 */
export function extractSessionId(output: string): string {
	const match = output.match(/([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})/i);
	if (!match) throw new Error(`Could not extract session ID from:\n${output.slice(0, 300)}`);
	return match[1];
}

/**
 * Extract an event ID (UUID) from search results.
 * Optionally specify which match (0-indexed) when multiple events are present.
 */
export function extractEventId(output: string, index = 0): string {
	const ids = extractAllEventIds(output);
	if (ids.length === 0) throw new Error(`Could not extract event ID from:\n${output.slice(0, 300)}`);
	if (index >= ids.length) throw new Error(`Requested event ID at index ${index} but only ${ids.length} found in:\n${output.slice(0, 300)}`);
	return ids[index];
}

/**
 * Extract a marker ID from overview/search output.
 */
export function extractMarkerId(output: string): string {
	const match = output.match(/([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})/i);
	if (!match) throw new Error(`Could not extract marker ID from:\n${output.slice(0, 300)}`);
	return match[1];
}

/**
 * Extract all event IDs from search/overview results as an array.
 */
export function extractAllEventIds(output: string): string[] {
	// First try id: <uuid> pattern (event IDs in search results)
	const idMatches: string[] = [];
	const re = new RegExp(EVENT_ID_RE.source, "gi");
	let m: RegExpExecArray | null;
	// biome-ignore lint/suspicious/noAssignInExpressions: regex loop
	while ((m = re.exec(output)) !== null) {
		idMatches.push(m[1]);
	}
	if (idMatches.length > 0) return idMatches;

	// Fall back to any UUIDs in the output
	const all = output.match(new RegExp(UUID_RE.source, "gi")) ?? [];
	return [...new Set(all)];
}

/**
 * Assert that an MCP tool result contains framework-specific content.
 * Provides better error messages than raw string matching.
 */
export function expectFrameworkContent(
	result: string,
	framework: "react" | "vue",
	expectations: {
		hasDetection?: boolean;
		hasStateEvents?: boolean;
		hasErrorEvents?: boolean;
		componentNames?: string[];
		patternNames?: string[];
	},
): void {
	const failures: string[] = [];
	const lower = result.toLowerCase();

	if (expectations.hasDetection) {
		if (!lower.includes(framework)) {
			failures.push(`Expected framework detection for '${framework}' but it was not found`);
		}
	}

	if (expectations.hasStateEvents) {
		if (!lower.includes("framework_state") && !lower.includes("state")) {
			failures.push("Expected framework state events but none found");
		}
	}

	if (expectations.hasErrorEvents) {
		if (!lower.includes("framework_error") && !lower.includes("error")) {
			failures.push("Expected framework error events but none found");
		}
	}

	for (const name of expectations.componentNames ?? []) {
		if (!result.includes(name)) {
			failures.push(`Expected component name '${name}' in output`);
		}
	}

	for (const pattern of expectations.patternNames ?? []) {
		if (!result.includes(pattern)) {
			failures.push(`Expected pattern name '${pattern}' in output`);
		}
	}

	if (failures.length > 0) {
		throw new Error(`expectFrameworkContent failed for framework='${framework}':\n${failures.map((f) => `  - ${f}`).join("\n")}\n\nActual output (first 500 chars):\n${result.slice(0, 500)}`);
	}
}

/**
 * Run the standard "find session → overview → search → inspect" sequence.
 * Returns all intermediate results for further assertions.
 */
export async function runInvestigationSequence(
	callTool: (name: string, args: Record<string, unknown>) => Promise<string>,
	options?: {
		searchFilters?: Record<string, unknown>;
		inspectIncludes?: string[];
	},
): Promise<{
	sessionId: string;
	listResult: string;
	overviewResult: string;
	searchResult: string;
	inspectResult: string;
}> {
	const listResult = await callTool("session_list", {});
	const sessionId = extractSessionId(listResult);

	const overviewResult = await callTool("session_overview", {
		session_id: sessionId,
		...options?.searchFilters,
	});

	const searchResult = await callTool("session_search", {
		session_id: sessionId,
		...(options?.searchFilters ?? {}),
	});

	let inspectResult = "";
	try {
		const eventId = extractEventId(searchResult);
		const inspectArgs: Record<string, unknown> = {
			session_id: sessionId,
			event_id: eventId,
		};
		if (options?.inspectIncludes) {
			inspectArgs.include = options.inspectIncludes;
		}
		inspectResult = await callTool("session_inspect", inspectArgs);
	} catch {
		// No events to inspect — not an error in the sequence
		inspectResult = "";
	}

	return { sessionId, listResult, overviewResult, searchResult, inspectResult };
}
