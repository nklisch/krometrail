import type { ViewportConfig, ViewportSnapshot } from "./types.js";

/**
 * Renders a ViewportSnapshot into the compact text format returned to agents.
 * See docs/UX.md for the viewport format specification.
 */
export function renderViewport(snapshot: ViewportSnapshot, config: ViewportConfig): string {
	const lines: string[] = [];

	// Header
	lines.push(`── STOPPED at ${snapshot.file}:${snapshot.line} (${snapshot.function}) ──`);
	lines.push(`Reason: ${snapshot.reason}`);
	lines.push("");

	// Call stack
	const frameCount = snapshot.stack.length;
	lines.push(`Call Stack (${frameCount} of ${snapshot.totalFrames} frames):`);
	for (let i = 0; i < frameCount; i++) {
		const f = snapshot.stack[i];
		const marker = i === 0 ? "→" : " ";
		lines.push(`  ${marker} ${f.shortFile}:${f.line}  ${f.function}(${f.arguments})`);
	}
	lines.push("");

	// Source
	if (snapshot.source.length > 0) {
		const start = snapshot.source[0].line;
		const end = snapshot.source[snapshot.source.length - 1].line;
		lines.push(`Source (${start}–${end}):`);
		for (const sl of snapshot.source) {
			const marker = sl.line === snapshot.line ? "→" : " ";
			lines.push(`${marker}${String(sl.line).padStart(4)}│ ${sl.text}`);
		}
		lines.push("");
	}

	// Locals
	if (snapshot.locals.length > 0) {
		const maxName = Math.max(...snapshot.locals.map((v) => v.name.length), 8);
		lines.push("Locals:");
		for (const v of snapshot.locals.slice(0, config.localsMaxItems)) {
			lines.push(`  ${v.name.padEnd(maxName)}  = ${v.value}`);
		}
		const remaining = snapshot.locals.length - config.localsMaxItems;
		if (remaining > 0) {
			lines.push(`  (${remaining} more...)`);
		}
	}

	// Watch expressions
	if (snapshot.watches && snapshot.watches.length > 0) {
		lines.push("");
		const maxExpr = Math.max(...snapshot.watches.map((w) => w.name.length), 8);
		lines.push("Watch:");
		for (const w of snapshot.watches) {
			lines.push(`  ${w.name.padEnd(maxExpr)}  = ${w.value}`);
		}
	}

	return lines.join("\n");
}
