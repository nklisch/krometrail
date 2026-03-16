import { getErrorMessage } from "../../core/errors.js";

/**
 * Shared MCP tool response helpers.
 */

export type ToolResult = { content: Array<{ type: "text"; text: string }>; isError?: true };

export function errorResponse(err: unknown): { content: Array<{ type: "text"; text: string }>; isError: true } {
	return { content: [{ type: "text" as const, text: getErrorMessage(err) }], isError: true };
}

export function textResponse(text: string): { content: Array<{ type: "text"; text: string }> } {
	return { content: [{ type: "text" as const, text }] };
}

/**
 * Wraps a simple async tool handler in a try/catch that returns errorResponse on failure.
 * Use for handlers where the entire logic is a single async call returning a string.
 */
export function toolHandler<T>(fn: (params: T) => Promise<string>): (params: T) => Promise<ToolResult> {
	return async (params) => {
		try {
			return textResponse(await fn(params));
		} catch (err) {
			return errorResponse(err);
		}
	};
}
