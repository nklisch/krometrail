import type { ViewportConfig } from "./types.js";

/**
 * Renders a DAP variable value into a compact string representation
 * following the viewport value rendering rules.
 *
 * - Primitives: as-is
 * - Strings: quoted, truncated
 * - Collections: type + length + preview
 * - Objects: type name + key fields
 */
export function renderValue(_value: unknown, _depth: number, _config: ViewportConfig): string {
	// TODO: implement value rendering
	throw new Error("Not implemented");
}
