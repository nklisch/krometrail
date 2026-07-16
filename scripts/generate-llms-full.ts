/**
 * Generates llms-full.txt from the practical Krometrail documentation.
 *
 * The public bundle is intentionally curated around installation, use, troubleshooting,
 * and current runtime behavior. Foundation, research, evidence archives, and contributor
 * mechanics remain available at their own URLs without crowding an agent's usage context.
 */

const HEADER = `# Krometrail Documentation

> Browser memory for coding agents: install Krometrail, use it to inspect transient browser behavior, and troubleshoot the local connection.

`;

const PUBLIC_DOCS = [
	"index.md",
	"guide/installation.md",
	"guide/using-krometrail.md",
	"guide/troubleshooting.md",
	"guide/mcp-configuration.md",
	"reference/runtime.md",
	"reference/configuration.md",
	"legal/privacy.md",
] as const;

function stripFrontmatter(content: string): string {
	return content.replace(/^---\n[\s\S]*?\n---\n?/, "");
}

async function main(): Promise<void> {
	const docsDir = new URL("../docs/", import.meta.url).pathname;
	const outPath = new URL("../docs/public/llms-full.txt", import.meta.url).pathname;
	const sections: string[] = [];

	for (const relPath of PUBLIC_DOCS) {
		const raw = await Bun.file(`${docsDir}${relPath}`).text();
		const stripped = stripFrontmatter(raw).trim();
		if (stripped.length > 0) {
			sections.push(stripped);
		}
	}

	const output = `${HEADER + sections.join("\n\n---\n\n")}\n`;

	await Bun.write(outPath, output);

	console.log(`Generated ${outPath}`);
	console.log(`  ${PUBLIC_DOCS.length} files included`);
}

if (import.meta.main) {
	main().catch((error) => {
		console.error("Error generating llms-full.txt:", error);
		process.exit(1);
	});
}
