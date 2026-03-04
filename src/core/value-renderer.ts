import type { DebugProtocol } from "@vscode/debugprotocol";
import type { Variable, ViewportConfig } from "./types.js";

/**
 * Configuration for rendering a single variable value.
 */
export interface RenderOptions {
	/** Current nesting depth (0 = top level). */
	depth: number;
	/** Maximum depth to render. Beyond this, show type summary only. */
	maxDepth: number;
	/** Maximum string length before truncation. */
	stringTruncateLength: number;
	/** Number of collection items to preview. */
	collectionPreviewItems: number;
}

/**
 * Python internal variable names to filter from the default locals display.
 */
export const PYTHON_INTERNAL_NAMES: ReadonlySet<string> = new Set(["__builtins__", "__doc__", "__name__", "__package__", "__spec__", "__loader__", "__file__", "__cached__", "__annotations__"]);

/**
 * JavaScript internal variable names to filter from the default locals display.
 */
export const JS_INTERNAL_NAMES: ReadonlySet<string> = new Set([
	"__proto__",
	"constructor",
	"__defineGetter__",
	"__defineSetter__",
	"__lookupGetter__",
	"__lookupSetter__",
	"hasOwnProperty",
	"isPrototypeOf",
	"propertyIsEnumerable",
	"toLocaleString",
	"toString",
	"valueOf",
]);

/**
 * Go internal variable names to filter.
 * Delve exposes runtime internals that are not useful for debugging.
 */
export const GO_INTERNAL_NAMES: ReadonlySet<string> = new Set(["runtime.curg", "runtime.frameoff", "&runtime.g"]);

/**
 * Generic internal variable name patterns to filter.
 * Matches names starting and ending with double underscores.
 */
export function isInternalVariable(name: string): boolean {
	return PYTHON_INTERNAL_NAMES.has(name) || JS_INTERNAL_NAMES.has(name) || GO_INTERNAL_NAMES.has(name) || /^__\w+__$/.test(name);
}

/**
 * Render a string value, adding quotes and truncating.
 */
export function renderString(value: string, maxLength: number): string {
	// debugpy returns strings already quoted, e.g. "'hello world'"
	// Strip outer quotes if present, then re-apply
	let inner = value;
	if ((inner.startsWith("'") && inner.endsWith("'")) || (inner.startsWith('"') && inner.endsWith('"'))) {
		inner = inner.slice(1, -1);
	}
	if (inner.length > maxLength) {
		return `"${inner.slice(0, maxLength)}..."`;
	}
	return `"${inner}"`;
}

/**
 * Render a collection value with type, length, and preview items.
 */
export function renderCollection(value: string, type: string, previewItems: number): string {
	const isDict = type === "dict" || type.startsWith("map[");
	const openBracket = isDict ? "{" : "[";
	const closeBracket = isDict ? "}" : "]";

	// Try to parse the inner content
	// debugpy returns e.g. "[1, 2, 3]" or "[1, 2, 3, ...]" or "{'a': 1}"
	const trimmed = value.trim();
	const inner = trimmed.slice(1, -1).trim();

	if (!inner || inner === "...") {
		return `${openBracket}${closeBracket} (0 items)`;
	}

	// Count items - split by top-level commas
	const items = splitTopLevel(inner);
	const totalItems = items.length;
	const hasEllipsis = items[items.length - 1]?.trim() === "...";
	const actualItems = hasEllipsis ? items.slice(0, -1) : items;

	if (totalItems === 0 || (totalItems === 1 && items[0].trim() === "")) {
		return `${openBracket}${closeBracket} (0 items)`;
	}

	const previewCount = Math.min(previewItems, actualItems.length);
	const preview = actualItems
		.slice(0, previewCount)
		.map((s) => s.trim())
		.join(", ");

	const itemCount = hasEllipsis ? `${actualItems.length}+` : String(totalItems);

	if (previewCount < actualItems.length || hasEllipsis) {
		return `${openBracket}${preview}, ... (${itemCount} items)${closeBracket}`;
	}
	if (totalItems > previewItems) {
		return `${openBracket}${preview}, ... (${totalItems} items)${closeBracket}`;
	}
	return `${openBracket}${preview}${closeBracket} (${totalItems} items)`;
}

/**
 * Split a string by top-level commas (not inside brackets/braces/parens).
 */
function splitTopLevel(s: string): string[] {
	const items: string[] = [];
	let depth = 0;
	let current = "";
	let inString = false;
	let stringChar = "";

	for (let i = 0; i < s.length; i++) {
		const ch = s[i];
		if (inString) {
			current += ch;
			if (ch === stringChar && s[i - 1] !== "\\") inString = false;
		} else if (ch === "'" || ch === '"') {
			inString = true;
			stringChar = ch;
			current += ch;
		} else if (ch === "(" || ch === "[" || ch === "{") {
			depth++;
			current += ch;
		} else if (ch === ")" || ch === "]" || ch === "}") {
			depth--;
			current += ch;
		} else if (ch === "," && depth === 0) {
			items.push(current);
			current = "";
		} else {
			current += ch;
		}
	}
	if (current.trim()) items.push(current);
	return items;
}

/**
 * Render an object/class instance value.
 */
export function renderObject(value: string, type: string, depth: number, maxDepth: number): string {
	if (depth >= maxDepth) {
		return `<${type}>`;
	}
	// debugpy returns objects like "<ClassName object at 0x...>" or just the repr
	// Try to extract meaningful content from the value
	// If it looks like a Python repr with attributes, show them; otherwise show type
	const cleaned = value.trim();

	// If the value is already a simple repr (not an address), use it compactly
	if (!cleaned.includes(" object at 0x") && !cleaned.startsWith("<") && cleaned.length < 80) {
		return `<${type}: ${cleaned}>`;
	}

	return `<${type}>`;
}

/**
 * Render a DAP variable into a compact string representation.
 */
export function renderDAPVariable(variable: DebugProtocol.Variable, options: RenderOptions): string {
	const type = variable.type ?? "";
	const value = variable.value ?? "";

	// NoneType
	if (type === "NoneType" || value === "None") {
		return "None";
	}

	// Primitives: int, float, bool
	if (type === "int" || type === "float" || type === "bool") {
		return value;
	}

	// Strings
	if (type === "str") {
		return renderString(value, options.stringTruncateLength);
	}

	// Collections: list, tuple, set, frozenset
	if (type === "list" || type === "tuple" || type === "set" || type === "frozenset") {
		return renderCollection(value, type, options.collectionPreviewItems);
	}

	// Dicts
	if (type === "dict") {
		return renderCollection(value, "dict", options.collectionPreviewItems);
	}

	// Arrays and other array-like types
	if (type.endsWith("Array") || type === "array") {
		return renderCollection(value, type, options.collectionPreviewItems);
	}

	// JavaScript types from js-debug
	if (type === "number" || type === "bigint") return value;
	if (type === "boolean") return value;
	if (type === "undefined") return "undefined";
	if (type === "null" || value === "null") return "null";
	if (type === "symbol") return value;
	if (type === "function") return `<function ${value.length > 40 ? `${value.slice(0, 40)}...` : value}>`;

	// Go types from Delve: slices, maps, pointers, structs
	// Go slices: []int, []string, etc.
	if (type.startsWith("[]")) return renderCollection(value, type, options.collectionPreviewItems);
	// Go maps: map[string]int, etc.
	if (type.startsWith("map[")) return renderCollection(value, type, options.collectionPreviewItems);
	// Go pointers: *main.Foo
	if (type.startsWith("*")) {
		const baseType = type.slice(1).replace(/^[a-z_]\w*\./, ""); // strip package prefix
		return renderObject(value, `*${baseType}`, options.depth, options.maxDepth);
	}
	// Go structs: main.User, pkg.Type — strip package prefix for display
	if (/^[a-z_]\w*\.[A-Z]/.test(type)) {
		const displayType = type.replace(/^[a-z_]\w*\./, "");
		return renderObject(value, displayType, options.depth, options.maxDepth);
	}

	// Objects with variablesReference > 0 (expandable)
	if (variable.variablesReference > 0) {
		return renderObject(value, type || "object", options.depth, options.maxDepth);
	}

	// Fallback: return value as-is, or type if value is empty
	return value || type || "?";
}

/**
 * Convert a DAP variable to our Variable type for the viewport.
 * Applies filtering (removes internal variables) and rendering.
 */
export function convertDAPVariables(dapVariables: DebugProtocol.Variable[], config: ViewportConfig): Variable[] {
	const options: RenderOptions = {
		depth: 0,
		maxDepth: config.localsMaxDepth,
		stringTruncateLength: config.stringTruncateLength,
		collectionPreviewItems: config.collectionPreviewItems,
	};

	return dapVariables
		.filter((v) => !isInternalVariable(v.name))
		.map((v) => ({
			name: v.name,
			value: renderDAPVariable(v, options),
			type: v.type,
		}));
}
