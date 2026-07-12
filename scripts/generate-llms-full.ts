/**
 * Generates llms-full.txt for the Krometrail docs site.
 *
 * Reads current contributor and reference pages, strips YAML frontmatter, and writes one
 * deterministic document. Foundation documents remain separately authoritative and are
 * linked from the generated guide rather than duplicated here.
 */

const HEADER = `# Krometrail Documentation

> Current contributor and runtime-reference documentation for Krometrail's Rust browser-capture foundation.

`;

const EXCLUDED_DIRS = [".vitepress", "node_modules"];
const FOUNDATION_DOCS = ["ARCHITECTURE.md", "EVALUATION.md", "SPEC.md", "VISION.md", "VISUAL-EVIDENCE.md", "agents.md"];

function stripFrontmatter(content: string): string {
	// Strip YAML frontmatter: opening ---, content, closing ---
	return content.replace(/^---\n[\s\S]*?\n---\n?/, "");
}

async function main(): Promise<void> {
	const docsDir = new URL("../docs/", import.meta.url).pathname;
	const outPath = new URL("../docs/public/llms-full.txt", import.meta.url).pathname;

	const glob = new Bun.Glob("**/*.md");
	const files: string[] = [];

	for await (const relPath of glob.scan({ cwd: docsDir, onlyFiles: true })) {
		// Exclude paths that start with any excluded directory
		const parts = relPath.split("/");
		if (parts.some((part) => EXCLUDED_DIRS.includes(part))) {
			continue;
		}
		// Keep foundation documents and navigation as separate sources of truth.
		if (FOUNDATION_DOCS.includes(relPath)) {
			continue;
		}
		files.push(relPath);
	}

	// Sort for deterministic output
	files.sort();

	const sections: string[] = [];

	for (const relPath of files) {
		const fullPath = `${docsDir}${relPath}`;
		const raw = await Bun.file(fullPath).text();
		const stripped = stripFrontmatter(raw).trim();
		if (stripped.length > 0) {
			sections.push(stripped);
		}
	}

	const output = `${HEADER + sections.join("\n\n---\n\n")}\n`;

	await Bun.write(outPath, output);

	console.log(`Generated ${outPath}`);
	console.log(`  ${files.length} files included`);
}

// Only run main when executed directly (not imported for tests)
if (import.meta.main) {
	main().catch((err) => {
		console.error("Error generating llms-full.txt:", err);
		process.exit(1);
	});
}
