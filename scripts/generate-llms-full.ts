/**
 * Generates llms-full.txt for the Krometrail docs site.
 *
 * Reads all .md files from docs/ (excluding .vitepress/, .generated/, designs/, legacy/, node_modules/),
 * strips YAML frontmatter, concatenates with --- separators, and writes to docs/public/llms-full.txt.
 */

const HEADER = `# Krometrail Documentation

> Complete documentation for Krometrail — runtime debugging and browser observation for AI coding agents.

`;

const EXCLUDED_DIRS = [".vitepress", ".generated", "designs", "legacy", "framework-state", "node_modules"];

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
		// Exclude root-level foundation docs
		if (["ARCH.md", "SPEC.md", "UX.md", "VISION.md", "PRIOR_ART.md", "ADAPTER-SDK.md", "agents.md"].includes(relPath)) {
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

	const output = HEADER + sections.join("\n\n---\n\n") + "\n";

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
