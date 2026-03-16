/**
 * Auto-generated docs script.
 *
 * Extracts documentation from Zod schemas and registries,
 * producing markdown partials that VitePress includes in reference pages.
 *
 * Output files (docs/.generated/):
 *   mcp-tools-debug.md    — 18 debug tools
 *   mcp-tools-browser.md  — 10 browser tools
 *   languages.md          — language adapter table
 *   frameworks.md         — framework detector table
 *   viewport-config.md    — viewport config parameter table
 */

import { mkdir, writeFile } from "node:fs/promises";
import type { z } from "zod";
import { listAdapters, registerAllAdapters } from "../src/adapters/registry.js";
import { listDetectors, registerAllDetectors } from "../src/frameworks/index.js";
import { registerBrowserTools } from "../src/mcp/tools/browser.js";
import { registerDebugTools } from "../src/mcp/tools/index.js";

// ---------------------------------------------------------------------------
// Captured tool type
// ---------------------------------------------------------------------------

export interface CapturedTool {
	name: string;
	description: string;
	params: Record<string, z.ZodTypeAny>;
}

export interface ExtractedParam {
	name: string;
	type: string;
	required: boolean;
	description: string;
}

// ---------------------------------------------------------------------------
// Mock MCP server that captures tool() calls
// ---------------------------------------------------------------------------

export function createCaptureMock(): { server: Record<string, unknown>; tools: CapturedTool[] } {
	const tools: CapturedTool[] = [];

	const server = {
		tool(name: string, ...rest: unknown[]) {
			// server.tool can be called with 2, 3, or 4 args:
			//   4 args: (name, description, schema, handler)
			//   3 args: (name, schema, handler) — no description
			if (rest.length >= 3) {
				// (name, description, schema, handler)
				tools.push({ name, description: rest[0] as string, params: rest[1] as Record<string, z.ZodTypeAny> });
			} else if (rest.length >= 2) {
				// (name, schema, handler)
				tools.push({ name, description: "", params: rest[0] as Record<string, z.ZodTypeAny> });
			}
		},
		resource: () => {},
		prompt: () => {},
	};

	return { server, tools };
}

// ---------------------------------------------------------------------------
// Zod 4 schema introspection
// ---------------------------------------------------------------------------

/**
 * Get the Zod 4 type name string from a schema's _def.type field.
 * Unwraps optional and default wrappers to get the inner type name.
 */
function getZodTypeName(schema: z.ZodTypeAny): string {
	const def = (schema as Record<string, Record<string, string>>)._def;
	if (!def) return "unknown";

	const typeName = def.type;

	if (typeName === "optional") {
		return getZodTypeName((def as unknown as { innerType: z.ZodTypeAny }).innerType);
	}
	if (typeName === "default") {
		return getZodTypeName((def as unknown as { innerType: z.ZodTypeAny }).innerType);
	}
	if (typeName === "string") return "string";
	if (typeName === "number") return "number";
	if (typeName === "boolean") return "boolean";
	if (typeName === "enum") {
		// Zod 4 uses _def.entries (an object/map), not _def.values (array)
		const entries = (def as unknown as { entries: Record<string, string> }).entries;
		if (entries && typeof entries === "object") {
			return Object.values(entries)
				.map((v) => `"${v}"`)
				.join(" \\| ");
		}
		return "enum";
	}
	if (typeName === "array") {
		const element = (def as unknown as { element: z.ZodTypeAny }).element;
		if (element) return `${getZodTypeName(element)}[]`;
		return "array";
	}
	if (typeName === "object") return "object";
	if (typeName === "union") {
		const options = (def as unknown as { options: z.ZodTypeAny[] }).options;
		if (options) return options.map((o) => getZodTypeName(o)).join(" \\| ");
		return "union";
	}
	if (typeName === "record") return "object";
	if (typeName === "literal") {
		const value = (def as unknown as { value: unknown }).value;
		return JSON.stringify(value);
	}
	return typeName ?? "unknown";
}

function isRequired(schema: z.ZodTypeAny): boolean {
	const def = (schema as Record<string, Record<string, string>>)._def;
	if (!def) return true;
	const typeName = def.type;
	return typeName !== "optional" && typeName !== "default";
}

function getDescription(schema: z.ZodTypeAny): string {
	return (schema as unknown as { description?: string }).description ?? "";
}

/**
 * Extract parameter info from a Zod schema object (the shape object passed to server.tool).
 * Exported for testability.
 */
export function extractParams(params: Record<string, z.ZodTypeAny>): ExtractedParam[] {
	return Object.entries(params).map(([name, schema]) => ({
		name,
		type: getZodTypeName(schema),
		required: isRequired(schema),
		description: getDescription(schema),
	}));
}

// ---------------------------------------------------------------------------
// Markdown generation helpers
// ---------------------------------------------------------------------------

function escapeTable(s: string): string {
	return s.replace(/\|/g, "\\|").replace(/\n/g, " ").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function escapeDescription(s: string): string {
	return s.replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function renderToolTable(params: Record<string, z.ZodTypeAny>): string {
	const extracted = extractParams(params);
	if (extracted.length === 0) {
		return "_No parameters._\n";
	}
	const rows = extracted.map((p) => `| \`${p.name}\` | ${escapeTable(p.type)} | ${p.required ? "Yes" : "No"} | ${escapeTable(p.description)} |`);
	return ["| Parameter | Type | Required | Description |", "|-----------|------|----------|-------------|", ...rows, ""].join("\n");
}

function renderToolSection(tool: CapturedTool): string {
	const lines: string[] = [];
	lines.push(`### \`${tool.name}\``);
	lines.push("");
	if (tool.description) {
		lines.push(escapeDescription(tool.description));
		lines.push("");
	}
	lines.push(renderToolTable(tool.params));
	return lines.join("\n");
}

// ---------------------------------------------------------------------------
// File generators
// ---------------------------------------------------------------------------

function generateDebugToolsMd(tools: CapturedTool[]): string {
	const debugTools = tools.filter((t) => t.name.startsWith("debug_"));
	return debugTools.map(renderToolSection).join("\n");
}

function generateBrowserToolsMd(tools: CapturedTool[]): string {
	const browserTools = tools.filter((t) => !t.name.startsWith("debug_"));
	return browserTools.map(renderToolSection).join("\n");
}

function generateLanguagesMd(): string {
	const adapters = listAdapters();
	const rows = adapters.map((a) => {
		const exts = a.fileExtensions.join(", ");
		const aliases = (a.aliases ?? []).join(", ");
		// Mark as active (all registered adapters are supported)
		return `| ${a.displayName} | \`${a.id}\` | \`${exts}\` | ${aliases || "—"} | Supported |`;
	});
	return ["| Language | ID | Extensions | Aliases | Status |", "|----------|----|------------|---------|--------|", ...rows, ""].join("\n");
}

function generateFrameworksMd(): string {
	const detectors = listDetectors();
	const rows = detectors.map((d) => `| ${d.displayName} | \`${d.adapterId}\` | \`${d.id}\` |`);
	return ["| Framework | Language Adapter | ID |", "|-----------|------------------|----|", ...rows, ""].join("\n");
}

async function generateViewportConfigMd(): Promise<string> {
	// The MCP tool uses snake_case; the core schema uses camelCase.
	// We hardcode the mapping here since ViewportConfigSchema (core) uses camelCase with defaults.
	// The MCP tool's ViewportConfigSchema uses snake_case optional fields.
	// We extract defaults from the core ViewportConfigSchema in src/core/types.ts.
	let ViewportConfigSchema: z.ZodTypeAny | null = null;
	try {
		const mod = await import("../src/core/types.js");
		ViewportConfigSchema = mod.ViewportConfigSchema ?? null;
	} catch {
		// ignore import errors — will use fallback defaults
	}

	// Fallback static data matching src/core/types.ts ViewportConfigSchema
	const fields: Array<{ name: string; camel: string; description: string; defaultValue: number }> = [
		{ name: "source_context_lines", camel: "sourceContextLines", description: "Lines of source shown above/below the current line", defaultValue: 15 },
		{ name: "stack_depth", camel: "stackDepth", description: "Max call stack frames shown", defaultValue: 5 },
		{ name: "locals_max_depth", camel: "localsMaxDepth", description: "Object expansion depth for local variables", defaultValue: 1 },
		{ name: "locals_max_items", camel: "localsMaxItems", description: "Max items shown per collection/object", defaultValue: 20 },
		{ name: "string_truncate_length", camel: "stringTruncateLength", description: "Max string length before truncation", defaultValue: 120 },
		{ name: "collection_preview_items", camel: "collectionPreviewItems", description: "Items shown in inline collection previews", defaultValue: 5 },
	];

	// Try to extract defaults from the actual schema
	if (ViewportConfigSchema) {
		const shape = (ViewportConfigSchema as unknown as { shape: Record<string, z.ZodTypeAny> }).shape;
		if (shape) {
			for (const field of fields) {
				const schema = shape[field.camel];
				if (schema) {
					const def = (schema as Record<string, Record<string, string>>)._def;
					if (def?.type === "default") {
						const rawDefault = (def as unknown as { defaultValue: unknown }).defaultValue;
						if (typeof rawDefault === "number") {
							field.defaultValue = rawDefault;
						} else if (typeof rawDefault === "function") {
							try {
								const v = (rawDefault as () => number)();
								if (typeof v === "number") field.defaultValue = v;
							} catch {
								// ignore
							}
						}
					}
				}
			}
		}
	}

	const rows = fields.map((f) => `| \`${f.name}\` | number | \`${f.defaultValue}\` | ${f.description} |`);
	return ["| Parameter | Type | Default | Description |", "|-----------|------|---------|-------------|", ...rows, ""].join("\n");
}

// ---------------------------------------------------------------------------
// CLI command docs generation
// ---------------------------------------------------------------------------

interface CliArgInfo {
	name: string;
	type: "positional" | "string" | "boolean";
	required: boolean;
	alias?: string;
	description: string;
	default?: unknown;
}

interface CliCommandInfo {
	name: string;
	description: string;
	group: string;
	args: CliArgInfo[];
}

/**
 * Global flags shared across debug commands (from shared.ts globalArgs).
 * These are excluded from per-command tables to reduce noise.
 */
const GLOBAL_FLAGS = new Set(["json", "quiet", "session"]);

function renderCliCommandTable(cmd: CliCommandInfo): string {
	const args = cmd.args.filter((a) => !GLOBAL_FLAGS.has(a.name));
	if (args.length === 0) return "";

	const rows = args.map((a) => {
		const flag = a.type === "positional" ? `\`<${a.name}>\`` : a.alias ? `\`--${a.name}\`, \`-${a.alias}\`` : `\`--${a.name}\``;
		const type = a.type === "positional" ? "positional" : a.type;
		const req = a.required ? "Yes" : "No";
		const desc = escapeTable(a.description);
		return `| ${flag} | ${type} | ${req} | ${desc} |`;
	});

	return ["| Flag | Type | Required | Description |", "|------|------|----------|-------------|", ...rows, ""].join("\n");
}

async function generateCliCommandDocs(outDir: string): Promise<number> {
	const { buildCommandInventory } = await import("../src/cli/commands/commands.js");
	const inventory = await buildCommandInventory();

	let fileCount = 0;
	for (const group of inventory.groups) {
		for (const cmd of group.commands) {
			const table = renderCliCommandTable(cmd as CliCommandInfo);
			if (!table) continue;

			const prefix = group.name === "top-level" ? "cli" : `cli-${group.name}`;
			const filename = `${prefix}-${cmd.name}.md`;
			await writeFile(`${outDir}${filename}`, table);
			fileCount++;
		}
	}
	return fileCount;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
	const outDir = new URL("../docs/.generated/", import.meta.url).pathname;
	await mkdir(outDir, { recursive: true });

	// 1. Populate registries (must be done before registerDebugTools/registerBrowserTools)
	registerAllAdapters();
	registerAllDetectors();

	// 2. Capture debug tool registrations
	const debugMock = createCaptureMock();
	registerDebugTools(debugMock.server as never, null as never);

	// 3. Capture browser tool registrations
	const browserMock = createCaptureMock();
	registerBrowserTools(browserMock.server as never, null as never);

	// 4. Generate MCP docs
	const debugMd = generateDebugToolsMd(debugMock.tools);
	const browserMd = generateBrowserToolsMd(browserMock.tools);
	const languagesMd = generateLanguagesMd();
	const frameworksMd = generateFrameworksMd();
	const viewportMd = await generateViewportConfigMd();

	await writeFile(`${outDir}mcp-tools-debug.md`, debugMd);
	await writeFile(`${outDir}mcp-tools-browser.md`, browserMd);
	await writeFile(`${outDir}languages.md`, languagesMd);
	await writeFile(`${outDir}frameworks.md`, frameworksMd);
	await writeFile(`${outDir}viewport-config.md`, viewportMd);

	// 5. Generate CLI command docs
	const cliFileCount = await generateCliCommandDocs(outDir);

	console.log("Generated docs in", outDir);
	console.log(`  mcp-tools-debug.md    (${debugMock.tools.filter((t) => t.name.startsWith("debug_")).length} tools)`);
	console.log(`  mcp-tools-browser.md  (${browserMock.tools.filter((t) => !t.name.startsWith("debug_")).length} tools)`);
	console.log(`  languages.md          (${listAdapters().length} adapters)`);
	console.log(`  frameworks.md         (${listDetectors().length} detectors)`);
	console.log(`  viewport-config.md    (6 params)`);
	console.log(`  cli-*.md              (${cliFileCount} CLI command tables)`);
}

// Only run main when executed directly (not imported for tests)
if (import.meta.main) {
	main().catch((err) => {
		console.error("Error generating docs:", err);
		process.exit(1);
	});
}
